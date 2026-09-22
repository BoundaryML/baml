//! Native local recording delivery. The shared publisher decides when to seal;
//! this sink writes its exact bytes, without reencoding or per-sink aggregation.
//!
//! A newly created recording directory has one writer. Completed files use
//! zero-padded `.btel` sequence names; `.btel.part` files are never complete records.
//! Files are written, closed and renamed without forcing storage synchronization.
//! Successful finish means every accepted file completed that procedure. Recent
//! data can be lost or damaged after a machine crash or power loss. It does not imply
//! `RecordingEnd`, settled clock validity, or complete metadata. Accepted CAS blobs
//! are written before later recording files on the same queue.
//!
//! A full queue blocks the delivery worker until capacity is available. The queue
//! bounds retained files and snapshot owners; the publisher seals on size or elapsed time.
//! Call finish after stopping the publisher; Drop only provides cleanup.
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread::JoinHandle,
};

use btel_publisher::{RecordingId, SealedFile};
pub use btel_settings::local_files::FileSinkConfig;

mod cas;
pub use cas::cas_path;
mod publisher;
pub use publisher::LocalPublisher;
mod reader;
pub use reader::{ReadIssue, RecordingRead, read_directory};

/// Terminal delivery errors retain the path and operating-system explanation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileSinkError(pub String);
impl fmt::Display for FileSinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for FileSinkError {}

pub fn recording_directory(root: &Path, id: RecordingId) -> PathBuf {
    let mut name = String::with_capacity(32);
    for byte in id.as_bytes() {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").expect("writing to String");
    }
    root.join(name)
}

fn io_error(path: &Path, error: &io::Error) -> FileSinkError {
    FileSinkError(format!("telemetry file {}: {error}", path.display()))
}

enum Command {
    File(SealedFile),
    Snapshot(btel_snapshot::Snapshot),
    Finish,
}

pub struct FileSender {
    sender: mpsc::SyncSender<Command>,
    result: Arc<OnceLock<Result<(), FileSinkError>>>,
}
impl FileSender {
    /// Called only after input chunk release. Full queues apply bounded backpressure
    /// to the delivery worker; disk errors and closed writers remain errors.
    pub fn send(&self, file: SealedFile) -> Result<(), FileSinkError> {
        self.enqueue(Command::File(file))
    }
    /// Move a frozen snapshot to the same bounded writer queue as recording files.
    pub fn send_snapshot(&self, snapshot: btel_snapshot::Snapshot) -> Result<(), FileSinkError> {
        self.enqueue(Command::Snapshot(snapshot))
    }
    fn enqueue(&self, command: Command) -> Result<(), FileSinkError> {
        if let Some(result) = self.result.get() {
            return result
                .clone()
                .and(Err(FileSinkError("telemetry file sink is closed".into())));
        }
        self.sender.send(command).map_err(|_| {
            if let Some(Err(failure)) = self.result.get() {
                return failure.clone();
            }
            FileSinkError("telemetry file writer stopped".into())
        })
    }
}

pub struct FileSink {
    directory: PathBuf,
    sender: mpsc::SyncSender<Command>,
    result: Arc<OnceLock<Result<(), FileSinkError>>>,
    delivery: Option<FileSender>,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl FileSink {
    /// Refuse an existing recording directory: sequences cannot overwrite a
    /// previous recording or race a second writer. Root directories may exist.
    pub fn create(root: &Path, id: RecordingId, config: FileSinkConfig) -> io::Result<Self> {
        Self::create_with_failure_handler(root, id, config, |_| {})
    }

    /// Notify the recording control independently of publisher progress. The
    /// error is retained before invoking this callback or releasing senders.
    pub fn create_with_failure_handler(
        root: &Path,
        id: RecordingId,
        config: FileSinkConfig,
        on_failure: impl FnOnce(&FileSinkError) + Send + 'static,
    ) -> io::Result<Self> {
        Self::create_with_cas(root, &root.join("cas"), id, config, on_failure)
    }

    /// Explicit shared CAS location. A recording owns its sequence directory;
    /// CAS blobs are shared across every recording using this project/home root.
    pub fn create_with_cas(
        root: &Path,
        cas_root: &Path,
        id: RecordingId,
        config: FileSinkConfig,
        on_failure: impl FnOnce(&FileSinkError) + Send + 'static,
    ) -> io::Result<Self> {
        if root.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty telemetry directory",
            ));
        }
        // Keep the writer bound to its original destination if the host later
        // changes its process working directory.
        let root = std::path::absolute(root)?;
        let cas_root = std::path::absolute(cas_root)?;
        fs::create_dir_all(&root)?;
        let directory = recording_directory(&root, id);
        fs::create_dir(&directory)?;
        let writer_directory = directory.clone();
        Self::start_with_snapshots(
            directory,
            config,
            move |file| write_file(&writer_directory, id, file),
            move |snapshot| cas::write_snapshot(&cas_root, snapshot),
            on_failure,
        )
    }

    #[cfg(any(test, feature = "test-support"))]
    fn start(
        directory: PathBuf,
        config: FileSinkConfig,
        write: impl FnMut(&SealedFile) -> Result<(), FileSinkError> + Send + 'static,
        on_failure: impl FnOnce(&FileSinkError) + Send + 'static,
    ) -> io::Result<Self> {
        let cas_root = directory.join("cas");
        Self::start_with_snapshots(
            directory,
            config,
            write,
            move |snapshot| cas::write_snapshot(&cas_root, snapshot),
            on_failure,
        )
    }

    fn start_with_snapshots(
        directory: PathBuf,
        config: FileSinkConfig,
        mut write: impl FnMut(&SealedFile) -> Result<(), FileSinkError> + Send + 'static,
        mut write_snapshot: impl FnMut(&btel_snapshot::Snapshot) -> Result<(), FileSinkError>
        + Send
        + 'static,
        on_failure: impl FnOnce(&FileSinkError) + Send + 'static,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(config.queue_files.get());
        let result = Arc::new(OnceLock::new());
        let worker_result = Arc::clone(&result);
        let worker = std::thread::Builder::new()
            .name("btel-file".into())
            .spawn(move || {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut next = Some(1_u64);
                    while let Ok(command) = receiver.recv() {
                        match command {
                            Command::Finish => return Ok(()),
                            Command::Snapshot(snapshot) => write_snapshot(&snapshot)?,
                            Command::File(file) => {
                                if next != Some(file.sequence().get()) {
                                    return Err(FileSinkError(
                                        "telemetry file sequence is not contiguous".into(),
                                    ));
                                }
                                write(&file)?;
                                next = file.sequence().get().checked_add(1);
                            }
                        }
                    }
                    Ok(())
                }))
                .unwrap_or_else(|_| Err(FileSinkError("telemetry file writer panicked".into())));
                let _ = worker_result.set(outcome);
                // Retain the result before releasing queued files and blocked sends.
                drop(receiver);
                if let Some(Err(error)) = worker_result.get() {
                    on_failure(error);
                }
            })?;
        Ok(Self {
            directory,
            delivery: Some(FileSender {
                sender: sender.clone(),
                result: Arc::clone(&result),
            }),
            sender,
            result,
            worker: Mutex::new(Some(worker)),
        })
    }

    /// Test-only writer substitution. Uses the real bounded queue, worker,
    /// shutdown and failure notification; only the storage operation is injected.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn with_test_writer(
        directory: PathBuf,
        config: FileSinkConfig,
        write: impl FnMut(&SealedFile) -> Result<(), FileSinkError> + Send + 'static,
        on_failure: impl FnOnce(&FileSinkError) + Send + 'static,
    ) -> io::Result<Self> {
        Self::start(directory, config, write, on_failure)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Move the sole delivery endpoint to the publisher. Cannot fan out or
    /// obtain a second endpoint for this recording.
    pub fn take_sender(&mut self) -> FileSender {
        self.delivery
            .take()
            .expect("recording already has its sole sender")
    }
    /// None while running. Disk errors are observable even without another file.
    pub fn result(&self) -> Option<Result<(), FileSinkError>> {
        self.result.get().cloned()
    }
    /// Blocking and idempotent. Stop the upstream publisher first. Runs outside
    /// VM execution/heap permits, typically in the engine's shutdown blocking task.
    pub fn finish(&self) -> Result<(), FileSinkError> {
        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = worker.take() {
            // Queued files precede the sentinel; a full queue also waits here.
            let _ = self.sender.send(Command::Finish);
            if handle.join().is_err() {
                let _ = self
                    .result
                    .set(Err(FileSinkError("telemetry file writer panicked".into())));
            }
        }
        self.result().unwrap_or_else(|| {
            Err(FileSinkError(
                "telemetry file writer stopped without a result".into(),
            ))
        })
    }
}
impl Drop for FileSink {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn write_file(directory: &Path, id: RecordingId, file: &SealedFile) -> Result<(), FileSinkError> {
    if file.recording_id() != id {
        return Err(FileSinkError(
            "telemetry file belongs to a different recording".into(),
        ));
    }
    let sequence = file.sequence().get();
    let destination = directory.join(format!("{sequence:020}.btel"));
    let temporary = directory.join(format!("{sequence:020}.btel.part"));
    // The fresh recording directory has one writer enforcing unique sequences,
    // so completed destinations cannot collide within this recording.
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| io_error(&temporary, &e))?;
    output
        .write_all(file.bytes())
        .map_err(|e| io_error(&temporary, &e))?;
    drop(output);
    fs::rename(&temporary, &destination).map_err(|e| io_error(&destination, &e))?;
    Ok(())
}

#[cfg(test)]
mod tests;
