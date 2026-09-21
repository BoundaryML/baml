//! Engine integration for the chunk processor and recording publisher.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use btel_publisher::{RecordingConfig, RecordingId, RecordingPublisher};

use crate::{BexEngine, EngineError, RuntimeCompiler};

/// One recording per engine, with exactly one local-file sink. BCS and spooling
/// are not implemented. Storage failures disable recording and remain available
/// through `BexEngine::telemetry_result`; they do not fail BAML execution.
pub struct TelemetryRecording {
    id: RecordingId,
    config: RecordingConfig,
    destination: Destination,
}
enum Destination {
    LocalFiles { recordings: PathBuf, cas: PathBuf },
    UserFiles,
}

impl TelemetryRecording {
    /// Write to `<project-root>/.baml/btel/recordings/<recording-id>`.
    /// Snapshots share `<project-root>/.baml/btel/cas/v1` across recordings.
    /// The caller supplies the resolved BAML project root; the engine does not
    /// rediscover it from the working directory or source paths. Packed binary
    /// hosts without a project root should use [`Self::user_files`].
    /// File output remains explicitly enabled.
    pub fn local_files(project_root: impl AsRef<Path>, config: RecordingConfig) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            destination: Destination::LocalFiles {
                recordings: project_root
                    .as_ref()
                    .join(btel_settings::local_files::RECORDINGS_DIRECTORY),
                cas: project_root
                    .as_ref()
                    .join(btel_settings::local_files::CAS_DIRECTORY),
            },
        }
    }

    /// Override the root: recordings use `<directory>/<recording-id>` and shared
    /// CAS uses `<directory>/cas/v1`. No I/O or worker
    /// starts until engine construction; `BAML_TELEMETRY=off` ignores this output.
    /// Existing recording directories are never reused. Shutdown awaits writes.
    pub fn local_files_in(directory: impl Into<PathBuf>, config: RecordingConfig) -> Self {
        let recordings = directory.into();
        let cas = recordings.join("cas");
        Self {
            id: RecordingId::generate(),
            config,
            destination: Destination::LocalFiles { recordings, cas },
        }
    }

    /// Write beneath the current user's home: `~/.baml/btel/recordings`.
    /// Packed programs share `~/.baml/btel/cas/v1` on this machine.
    /// Resolve the home directory only when the enabled engine starts recording.
    /// Fail explicitly if no home directory can be determined; never fall back
    /// to the working directory. Intended for packed programs without a project.
    pub fn user_files(config: RecordingConfig) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            destination: Destination::UserFiles,
        }
    }

    pub fn id(&self) -> RecordingId {
        self.id
    }

    pub(crate) fn start(
        self,
        source_snapshot: Option<[u8; 32]>,
    ) -> Result<
        (
            Arc<btel_processor::TelemetryRuntime>,
            Option<Arc<btel_file::FileSink>>,
        ),
        EngineError,
    > {
        let transport = btel_settings::transport::ChunkConfig::default();
        self.config
            .validate_transport(&transport)
            .map_err(|error| EngineError::Other(error.to_owned()))?;
        let root = match self.destination {
            Destination::UserFiles => std::env::home_dir()
                .map(|home| {
                    (
                        home.join(btel_settings::local_files::RECORDINGS_DIRECTORY),
                        home.join(btel_settings::local_files::CAS_DIRECTORY),
                    )
                })
                .ok_or_else(|| std::io::Error::other("cannot determine telemetry home directory")),
            Destination::LocalFiles { recordings, cas } => Ok((recordings, cas)),
        };
        let mut sink = None;
        let runtime =
            btel_processor::TelemetryRuntime::with_publisher_factory(transport, |control| {
                let failure = control.clone();
                let writer = root.and_then(|(root, cas)| {
                    btel_file::FileSink::create_with_cas(
                        &root,
                        &cas,
                        self.id,
                        btel_file::FileSinkConfig::default(),
                        move |error| {
                            failure.disable(btel_processor::RuntimeError(error.to_string()));
                            tracing::warn!(%error, "telemetry recording disabled");
                        },
                    )
                });
                let sender = match writer {
                    Ok(mut writer) => {
                        let sender = writer.take_sender();
                        sink = Some(Arc::new(writer));
                        Some(Arc::new(sender))
                    }
                    Err(error) => {
                        // Failure to create the directory/file worker also must not
                        // prevent application startup. No automatic retry.
                        tracing::warn!(%error, "telemetry recording startup disabled");
                        control.disable(btel_processor::RuntimeError(format!(
                            "telemetry file startup: {error}"
                        )));
                        None
                    }
                };
                let snapshot_sender = sender.clone();
                let snapshot_control = control.clone();
                RecordingPublisher::with_snapshot_receiver(
                    self.id,
                    self.config,
                    move |file| {
                        if let Some(sender) = &sender {
                            if let Err(error) = sender.send(file) {
                                control.disable(btel_processor::RuntimeError(error.to_string()));
                            }
                        }
                    },
                    move |snapshot| {
                        if let Some(sender) = &snapshot_sender {
                            if let Err(error) = sender.send_snapshot(snapshot) {
                                snapshot_control
                                    .disable(btel_processor::RuntimeError(error.to_string()));
                            }
                        }
                    },
                )
                .map(|publisher| publisher.with_source_snapshot(source_snapshot))
                .map_err(std::io::Error::other)
            })
            .map_err(|error| EngineError::Other(format!("telemetry processor startup: {error}")))?;
        Ok((runtime, sink))
    }
}

impl BexEngine {
    /// Configure encoded-file delivery before initialization executes. Existing
    /// constructors run the diagnostic processor without creating a recording.
    /// `BAML_TELEMETRY=off` disables it even when a recording is supplied.
    pub fn new_with_telemetry_recording(
        program: bex_vm_types::Program,
        sys_ops: Arc<sys_ops::SysOps>,
        argv: Vec<String>,
        runtime_compiler: Option<Arc<dyn RuntimeCompiler>>,
        clock_mode: btel_clock::ClockMode,
        recording: TelemetryRecording,
    ) -> Result<Self, EngineError> {
        Self::build(
            program,
            sys_ops,
            argv,
            runtime_compiler,
            clock_mode,
            Some(recording),
        )
    }

    /// No recording is created when `BAML_TELEMETRY=off`.
    pub fn telemetry_recording_id(&self) -> Option<RecordingId> {
        self.telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.recording_id)
    }

    /// Local output directory, when configured and telemetry is enabled.
    pub fn telemetry_recording_directory(&self) -> Option<&Path> {
        self.telemetry
            .as_ref()?
            .file_sink
            .as_ref()
            .map(|sink| sink.directory())
    }

    /// None when telemetry is off or while processing. A disabled recording returns
    /// its retained error. After shutdown, Some(Ok(())) means all published
    /// chunks were consumed and accepted files written and closed. Storage failures
    /// disable recording independently of application execution. It does not assert
    /// complete captures, final clock validity, or a `RecordingEnd` marker.
    pub fn telemetry_result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        self.telemetry
            .as_ref()
            .and_then(super::telemetry_state::EngineTelemetry::result)
    }
}

#[cfg(all(test, unix))]
mod failure_tests;
