//! Engine integration for the chunk processor and recording publisher.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use btel_publisher::{RecordingBuilder, RecordingConfig, RecordingId};

use crate::{BexEngine, EngineError, RuntimeCompiler};

/// One recording per engine, delivered to local files or BCS. Cloud payload
/// failures discard that payload and allow later delivery. Local storage and
/// fatal worker failures disable recording. Errors remain available through
/// `BexEngine::telemetry_result`; they do not fail BAML execution.
pub struct TelemetryRecording {
    id: RecordingId,
    config: RecordingConfig,
    destination: Destination,
}
enum Destination {
    LocalFiles {
        recordings: PathBuf,
        cas: PathBuf,
    },
    UserFiles,
    Cloud {
        publisher: btel_bcs::PublisherConfig,
        delivery: btel_bcs::delivery::DeliveryConfig,
    },
}

#[derive(Clone)]
pub(crate) enum RecordingDelivery {
    Local(Arc<btel_file::FileSink>),
    Cloud(Arc<btel_bcs::delivery::BcsDelivery>),
}

impl RecordingDelivery {
    pub(crate) fn finish(&self) -> Result<(), btel_processor::RuntimeError> {
        match self {
            Self::Local(sink) => sink
                .finish()
                .map_err(|error| btel_processor::RuntimeError(error.to_string())),
            Self::Cloud(delivery) => delivery
                .finish()
                .map_err(|error| btel_processor::RuntimeError(error.to_string())),
        }
    }

    pub(crate) fn result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        match self {
            Self::Local(sink) => sink
                .result()
                .map(|result| result.map_err(|e| btel_processor::RuntimeError(e.to_string()))),
            Self::Cloud(delivery) => delivery
                .result()
                .map(|result| result.map_err(|e| btel_processor::RuntimeError(e.to_string()))),
        }
    }

    fn directory(&self) -> Option<&Path> {
        match self {
            Self::Local(sink) => Some(sink.directory()),
            Self::Cloud(_) => None,
        }
    }

    fn heartbeat_error(&self) -> Option<btel_bcs::liveness::HeartbeatError> {
        match self {
            Self::Local(_) => None,
            Self::Cloud(delivery) => delivery.heartbeat_error(),
        }
    }
}

impl TelemetryRecording {
    /// Deliver through the proposed BCS prepare protocol and presigned PUTs.
    /// No worker starts until engine construction. There is no durable spool.
    pub fn cloud(
        config: RecordingConfig,
        publisher: btel_bcs::PublisherConfig,
        delivery: btel_bcs::delivery::DeliveryConfig,
    ) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            destination: Destination::Cloud {
                publisher,
                delivery,
            },
        }
    }

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
            Option<RecordingDelivery>,
        ),
        EngineError,
    > {
        let transport = btel_settings::transport::ChunkConfig::default();
        self.config
            .validate_transport(&transport)
            .map_err(|error| EngineError::Other(error.to_owned()))?;
        let root = match self.destination {
            Destination::Cloud {
                publisher,
                delivery,
            } => {
                return Self::start_cloud(
                    self.id,
                    self.config,
                    publisher,
                    delivery,
                    source_snapshot,
                    transport,
                );
            }
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
                let builder = RecordingBuilder::new(self.id, self.config)
                    .map_err(std::io::Error::other)?
                    .with_source_snapshot(source_snapshot);
                let publisher = match writer {
                    Ok(writer) => {
                        let publisher = btel_file::LocalPublisher::new(builder, writer);
                        sink = publisher.sink().cloned();
                        publisher
                    }
                    Err(error) => {
                        // Failure to create the directory/file worker also must not
                        // prevent application startup. No automatic retry.
                        tracing::warn!(%error, "telemetry recording startup disabled");
                        control.disable(btel_processor::RuntimeError(format!(
                            "telemetry file startup: {error}"
                        )));
                        btel_file::LocalPublisher::disabled(builder)
                    }
                };
                Ok(publisher)
            })
            .map_err(|error| EngineError::Other(format!("telemetry processor startup: {error}")))?;
        Ok((runtime, sink.map(RecordingDelivery::Local)))
    }

    fn start_cloud(
        id: RecordingId,
        config: RecordingConfig,
        publisher_config: btel_bcs::PublisherConfig,
        delivery_config: btel_bcs::delivery::DeliveryConfig,
        source_snapshot: Option<[u8; 32]>,
        transport: btel_settings::transport::ChunkConfig,
    ) -> Result<
        (
            Arc<btel_processor::TelemetryRuntime>,
            Option<RecordingDelivery>,
        ),
        EngineError,
    > {
        let mut delivery = None;
        let runtime =
            btel_processor::TelemetryRuntime::with_publisher_factory(transport, |control| {
                let failure = control.clone();
                let handle =
                    match btel_bcs::delivery::BcsDelivery::new(delivery_config, move |error| {
                        failure.disable(btel_processor::RuntimeError(error.to_string()));
                        tracing::warn!(%error, "cloud telemetry disabled");
                    }) {
                        Ok(worker) => {
                            let handle = worker.handle();
                            delivery = Some(RecordingDelivery::Cloud(Arc::new(worker)));
                            handle
                        }
                        Err(error) => {
                            control.disable(btel_processor::RuntimeError(error.to_string()));
                            btel_bcs::delivery::DeliveryHandle::disabled(error)
                        }
                    };
                btel_bcs::CloudPublisher::new(id, config, publisher_config, handle)
                    .map(|publisher| publisher.with_source_snapshot(source_snapshot))
                    .map_err(std::io::Error::other)
            })
            .map_err(|error| EngineError::Other(format!("telemetry processor startup: {error}")))?;
        Ok((runtime, delivery))
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
            .delivery
            .as_ref()
            .and_then(RecordingDelivery::directory)
    }

    /// `None` when telemetry is off or processing without a known error.
    /// Cloud payload loss returns a retained error even if later uploads succeed;
    /// an error does not necessarily mean recording is disabled.
    /// After shutdown, `Some(Ok(()))` means all published
    /// chunks were consumed and accepted delivery completed. Cloud PUT success
    /// does not imply ingestion or query availability. Local storage failures
    /// disable recording independently of application execution. It does not assert
    /// complete captures, final clock validity, or a `RecordingEnd` marker.
    pub fn telemetry_result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        self.telemetry
            .as_ref()
            .and_then(super::telemetry_state::EngineTelemetry::result)
    }

    /// Number of discarded cloud prepare groups or upload targets, not events or
    /// retry attempts. Zero for local recording or when telemetry is off.
    pub fn telemetry_delivery_loss_count(&self) -> u64 {
        match self
            .telemetry
            .as_ref()
            .and_then(|state| state.delivery.as_ref())
        {
            Some(RecordingDelivery::Cloud(delivery)) => delivery.handle().loss_count(),
            _ => 0,
        }
    }

    /// Metadata segments evicted from bounded cloud replay storage. An eviction
    /// is not confirmed upload loss: an in-flight copy may still succeed.
    pub fn telemetry_metadata_replay_evictions(&self) -> u64 {
        match self
            .telemetry
            .as_ref()
            .and_then(|state| state.delivery.as_ref())
        {
            Some(RecordingDelivery::Cloud(delivery)) => {
                delivery.handle().metadata_replay_evictions()
            }
            _ => 0,
        }
    }

    /// Last advisory heartbeat error, independent of recording delivery success.
    pub fn telemetry_heartbeat_error(&self) -> Option<btel_bcs::liveness::HeartbeatError> {
        self.telemetry
            .as_ref()?
            .delivery
            .as_ref()
            .and_then(RecordingDelivery::heartbeat_error)
    }
}

#[cfg(all(test, unix))]
mod failure_tests;
