//! Native local recording delivery. The shared publisher decides when to seal;
//! this sink writes its exact bytes, without reencoding or per-sink aggregation.
//!
//! A newly created recording directory has one writer. Completed files use
//! zero-padded sequence names; `.part` files are never complete records. File
//! data is synced before rename, and the directory is synced on Unix. Successful
//! finish means every accepted file completed that procedure. It does not imply
//! `RecordingEnd`, settled clock validity, available captures, or complete metadata.
//!
//! Queue admission never waits on disk. A full queue fails explicitly; retained
//! `SealedFile`s remain charged to the publisher's existing resident-byte budget.
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
    Finish,
}

#[derive(Clone)]
pub struct FileSender {
    sender: mpsc::SyncSender<Command>,
    result: Arc<OnceLock<Result<(), FileSinkError>>>,
}
impl FileSender {
    /// Called only after input chunk release. Full/failed/closed delivery never
    /// masquerades as success; callers must propagate the error, not drop it.
    pub fn send(&self, file: SealedFile) -> Result<(), FileSinkError> {
        if let Some(result) = self.result.get() {
            return result
                .clone()
                .and(Err(FileSinkError("telemetry file sink is closed".into())));
        }
        self.sender.try_send(Command::File(file)).map_err(|error| {
            if let Some(Err(failure)) = self.result.get() {
                return failure.clone();
            }
            FileSinkError(match error {
                mpsc::TrySendError::Full(_) => "telemetry file queue is full".into(),
                mpsc::TrySendError::Disconnected(_) => "telemetry file writer stopped".into(),
            })
        })
    }
}

pub struct FileSink {
    directory: PathBuf,
    sender: FileSender,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl FileSink {
    /// Refuse an existing recording directory: sequences cannot overwrite a
    /// previous recording or race a second writer. Root directories may exist.
    pub fn create(root: &Path, id: RecordingId, config: FileSinkConfig) -> io::Result<Self> {
        if root.as_os_str().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty telemetry directory",
            ));
        }
        fs::create_dir_all(root)?;
        let directory = recording_directory(root, id);
        fs::create_dir(&directory)?;
        #[cfg(unix)]
        fs::File::open(root)?.sync_all()?;
        let writer_directory = directory.clone();
        Self::start(directory, config, move |file| {
            write_file(&writer_directory, id, file)
        })
    }

    fn start(
        directory: PathBuf,
        config: FileSinkConfig,
        mut write: impl FnMut(&SealedFile) -> Result<(), FileSinkError> + Send + 'static,
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
            })?;
        Ok(Self {
            directory,
            sender: FileSender { sender, result },
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn sender(&self) -> FileSender {
        self.sender.clone()
    }
    /// None while running. Disk errors are observable even without another file.
    pub fn result(&self) -> Option<Result<(), FileSinkError>> {
        self.sender.result.get().cloned()
    }
    /// Blocking and idempotent. Stop the upstream publisher first. Runs outside
    /// VM execution/heap permits, typically in the engine's shutdown blocking task.
    pub fn finish(&self) -> Result<(), FileSinkError> {
        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(handle) = worker.take() {
            // Blocking only at shutdown; queued files precede the sentinel.
            let _ = self.sender.sender.send(Command::Finish);
            if handle.join().is_err() {
                let _ = self
                    .sender
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
    let destination = directory.join(format!("{sequence:020}.pb"));
    let temporary = directory.join(format!("{sequence:020}.pb.part"));
    if destination
        .try_exists()
        .map_err(|e| io_error(&destination, &e))?
    {
        return Err(FileSinkError(format!(
            "telemetry file already exists: {}",
            destination.display()
        )));
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| io_error(&temporary, &e))?;
    output
        .write_all(file.bytes())
        .and_then(|()| output.sync_all())
        .map_err(|e| io_error(&temporary, &e))?;
    drop(output);
    fs::rename(&temporary, &destination).map_err(|e| io_error(&destination, &e))?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|e| io_error(directory, &e))?;
    Ok(())
}

#[cfg(test)]
mod tests;
