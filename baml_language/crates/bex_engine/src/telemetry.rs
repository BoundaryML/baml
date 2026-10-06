//! Engine integration for the chunk processor and recording publisher.
use std::{
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use bcs_api::diagnostics::{
    Context as AuthorizationContext, CredentialSource, Operation, Outcome, RecordingOverride,
};
use btel_recorder::{RecordingBuilder, RecordingConfig, RecordingId};
use btel_settings::artifact::{ArtifactTelemetry, InitialFailureAction, InitialFailurePolicy};

use crate::{BexEngine, EngineError, RuntimeCompiler};

const BOUNDARY_API_KEY: &str = "BOUNDARY_API_KEY";

/// One recording per engine, delivered to local files or BCS. Cloud payload
/// failures discard that payload and allow later delivery. Initial cloud authorization
/// rejection cancels execution; later revocation only disables recording. Local storage
/// and fatal worker failures disable recording. Delivery errors remain available through
/// `BexEngine::telemetry_result` and initial refusal through
/// `BexEngine::initial_cloud_authorization_error`.
pub struct TelemetryRecording {
    id: RecordingId,
    config: RecordingConfig,
    destination: Destination,
    host: String,
    sources: Vec<(String, String)>,
    exit: Arc<btel_types::ProcessExitSlot>,
    initial_failure: Option<InitialFailurePolicy>,
    recording_override: RecordingOverride,
}
enum Destination {
    InvalidConfiguration(Box<bcs_api::diagnostics::Diagnostic>),
    LocalFiles {
        recordings: PathBuf,
        cas: PathBuf,
    },
    UserFiles,
    Cloud {
        publisher: btel_bcs::CloudPublisherConfig,
        delivery: Box<btel_bcs::delivery::DeliveryConfig>,
        authorization_context: AuthorizationContext,
    },
}

#[derive(Clone)]
pub(crate) enum RecordingDelivery {
    Local(Arc<btel_file::LocalDelivery>),
    Cloud {
        delivery: Arc<btel_bcs::delivery::BcsDelivery>,
        initial_auth_cancel: crate::CancellationToken,
        authorization_context: AuthorizationContext,
        initial_auth_error: Arc<OnceLock<EngineError>>,
        initial_failure: InitialFailurePolicy,
        recording_override: RecordingOverride,
    },
}

impl RecordingDelivery {
    pub(crate) fn finish(&self) -> Result<(), btel_processor::RuntimeError> {
        match self {
            Self::Local(delivery) => delivery
                .finish()
                .map_err(|error| btel_processor::RuntimeError(error.to_string())),
            Self::Cloud { delivery, .. } => delivery
                .finish()
                .map_err(|error| btel_processor::RuntimeError(error.to_string())),
        }
    }

    pub(crate) fn result(&self) -> Option<Result<(), btel_processor::RuntimeError>> {
        match self {
            Self::Local(delivery) => delivery
                .result()
                .map(|result| result.map_err(|e| btel_processor::RuntimeError(e.to_string()))),
            Self::Cloud { delivery, .. } => delivery
                .result()
                .map(|result| result.map_err(|e| btel_processor::RuntimeError(e.to_string()))),
        }
    }

    pub(crate) fn initial_auth_cancel(&self) -> Option<&crate::CancellationToken> {
        match self {
            Self::Cloud {
                initial_auth_cancel,
                ..
            } => Some(initial_auth_cancel),
            Self::Local(_) => None,
        }
    }

    fn initial_auth_error(&self) -> Option<EngineError> {
        match self {
            Self::Cloud {
                delivery,
                authorization_context,
                initial_auth_error,
                initial_failure,
                recording_override,
                ..
            } if initial_failure.action == InitialFailureAction::Abort => {
                match delivery.initial_authorization() {
                    btel_bcs::delivery::InitialAuthorization::Rejected(failure) => Some(
                        initial_auth_error
                            .get_or_init(|| {
                                EngineError::CloudAuthorization(Box::new(
                                    authorization_context
                                        .rejected_failure(&failure, Outcome::ExecutionCancelled)
                                        .with_recording_override(recording_override.clone()),
                                ))
                            })
                            .clone(),
                    ),
                    btel_bcs::delivery::InitialAuthorization::Failed(error) => Some(
                        initial_auth_error
                            .get_or_init(|| {
                                EngineError::CloudAuthorization(Box::new(
                                    initial_diagnostic(
                                        authorization_context,
                                        &error,
                                        Outcome::ExecutionCancelled,
                                    )
                                    .with_recording_override(recording_override.clone()),
                                ))
                            })
                            .clone(),
                    ),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub(crate) fn initial_failure_handled(&self) -> bool {
        matches!(self, Self::Cloud { delivery, initial_failure, .. }
            if initial_failure.action != InitialFailureAction::Abort
                && matches!(delivery.initial_authorization(), btel_bcs::delivery::InitialAuthorization::Rejected(_) | btel_bcs::delivery::InitialAuthorization::Failed(_)))
    }

    fn directory(&self) -> Option<&Path> {
        match self {
            Self::Local(delivery) => Some(delivery.directory()),
            Self::Cloud { .. } => None,
        }
    }

    fn heartbeat_error(&self) -> Option<btel_bcs::liveness::HeartbeatError> {
        match self {
            Self::Local(_) => None,
            Self::Cloud { delivery, .. } => delivery.heartbeat_error(),
        }
    }
}

impl TelemetryRecording {
    /// Resolve an artifact's explicitly permitted bindings without changing process
    /// environment. Embedded credentials never fall back to an ambient login or key.
    pub fn from_artifact(policy: &ArtifactTelemetry) -> Result<Self, EngineError> {
        use btel_settings::artifact::IngestionSelection;
        if policy
            .resolve_level()
            .map_err(|error| EngineError::Other(error.to_string()))?
            .is_none()
        {
            return Ok(Self::user_files(RecordingConfig::default()));
        }
        let selection = policy
            .select_ingestion_with(baml_env::os_var)
            .map_err(|error| EngineError::Other(error.to_string()))?;
        if matches!(&selection, IngestionSelection::Caller { api_key: Some(key), .. } if key == bcs_api::credentials::LOCAL_API_KEY)
        {
            return Ok(Self::user_files(RecordingConfig::default()));
        }
        let endpoint =
            bcs_api::Endpoint::with_default(policy.api_url.as_deref()).map_err(|error| {
                artifact_configuration_error(policy, None, CredentialSource::Configured, error)
            })?;
        let (credential, target, source, build_id) = match selection {
            IngestionSelection::Embedded(embedded) => (
                embedded.token.expose().to_owned(),
                bcs_api::credentials::Target {
                    org_id: Some(embedded.destination.org_id.clone()),
                    project_id: Some(embedded.destination.project_id.clone()),
                    environment_id: Some(embedded.destination.environment_id.clone()),
                    ..Default::default()
                },
                CredentialSource::Embedded,
                Some(embedded.build_id.clone()),
            ),
            IngestionSelection::Caller { project, api_key } => {
                let (credential, source) = match api_key {
                    Some(key) => {
                        if key.starts_with(bcs_api::credentials::PUBLIC_PREFIX) {
                            return Err(EngineError::Other("A public telemetry credential must be embedded with its build registration. Supply a Boundary API key or use `baml auth login`.".into()));
                        }
                        (key, CredentialSource::ApiKey)
                    }
                    None => {
                        let saved =
                            match bcs_api::Store::new(&endpoint).and_then(|store| store.read()) {
                                Ok(saved) => saved,
                                Err(error) => {
                                    return Err(artifact_configuration_error(
                                        policy,
                                        Some(endpoint),
                                        CredentialSource::SavedLogin,
                                        error,
                                    ));
                                }
                            };
                        match saved {
                            Some(saved) => (
                                saved.refresh_token.expose().to_owned(),
                                CredentialSource::SavedLogin,
                            ),
                            None if policy.embedded.is_none() => {
                                return Ok(Self::user_files(RecordingConfig::default()));
                            }
                            None => {
                                return Err(artifact_configuration_error(
                                    policy,
                                    Some(endpoint),
                                    CredentialSource::SavedLogin,
                                    bcs_api::Error::MissingCredentials,
                                ));
                            }
                        }
                    }
                };
                (
                    credential,
                    bcs_api::credentials::Target {
                        project,
                        ..Default::default()
                    },
                    source,
                    None,
                )
            }
        };
        let authentication = match build_id {
            Some(build) => bcs_api::credentials::Authentication::for_build(
                endpoint.as_str(),
                bcs_api::Secret::new(credential),
                build,
            ),
            None => bcs_api::credentials::Authentication::shared(
                endpoint.as_str(),
                bcs_api::Secret::new(credential),
            ),
        };
        let mut delivery = btel_bcs::delivery::DeliveryConfig::new(
            endpoint
                .as_str()
                .parse()
                .map_err(|_| EngineError::Other("Invalid Boundary API URL.".into()))?,
        );
        delivery.authorization = Some(bcs_api::credentials::RequestAuthorization {
            authentication,
            target: Some(target),
        });
        let mut recording = Self::cloud(
            RecordingConfig::default(),
            btel_bcs::CloudPublisherConfig::default(),
            delivery,
        );
        if let Destination::Cloud {
            authorization_context,
            ..
        } = &mut recording.destination
        {
            authorization_context.source = source;
        }
        Ok(recording)
    }

    /// Resolve the selected endpoint and credential without making a network request.
    pub fn from_boundary_env() -> Option<Self> {
        if baml_env::string_var(BOUNDARY_API_KEY).ok()?.as_deref()
            == Some(bcs_api::credentials::LOCAL_API_KEY)
        {
            return Some(Self::user_files(RecordingConfig::default()));
        }
        Self::from_boundary_defaults(None, None)
    }

    /// Hosts pass artifact/project defaults; process environment variables take precedence.
    /// Invalid Boundary configuration cancels engine startup with a shared diagnostic.
    /// `None` selects the host's local
    /// default, either because there is no cloud credential or the key is `local`.
    pub fn from_boundary_defaults(project: Option<&str>, api_url: Option<&str>) -> Option<Self> {
        let api_key = baml_env::string_var(BOUNDARY_API_KEY).ok()?;
        if api_key.as_deref() == Some(bcs_api::credentials::LOCAL_API_KEY) {
            return None;
        }
        let endpoint = match bcs_api::Endpoint::with_default(api_url) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let mut recording = Self::user_files(RecordingConfig::default());
                recording.destination = Destination::InvalidConfiguration(Box::new(
                    AuthorizationContext {
                        endpoint: None,
                        source: CredentialSource::Configured,
                        operation: Operation::Ingest,
                    }
                    .report(error, Outcome::ExecutionCancelled)
                    .diagnostic,
                ));
                return Some(recording);
            }
        };
        let source = if api_key.is_some() {
            CredentialSource::ApiKey
        } else {
            CredentialSource::SavedLogin
        };
        let key = match api_key {
            Some(key) => key,
            None => match bcs_api::Store::new(&endpoint).and_then(|store| store.read()) {
                Ok(Some(saved)) => saved.refresh_token.expose().to_owned(),
                Ok(None) => return None,
                Err(error) => {
                    let mut recording = Self::user_files(RecordingConfig::default());
                    recording.destination = Destination::InvalidConfiguration(Box::new(
                        AuthorizationContext {
                            endpoint: Some(endpoint),
                            source: CredentialSource::SavedLogin,
                            operation: Operation::Ingest,
                        }
                        .report(error, Outcome::ExecutionCancelled)
                        .diagnostic,
                    ));
                    return Some(recording);
                }
            },
        };
        let target = bcs_api::credentials::Target {
            project: baml_env::string_var("BOUNDARY_PROJECT")
                .ok()?
                .or_else(|| project.map(str::to_owned)),
            ..Default::default()
        };
        let mut delivery = btel_bcs::delivery::DeliveryConfig::new(endpoint.as_str().parse().ok()?);
        delivery.authorization = Some(bcs_api::credentials::RequestAuthorization {
            authentication: bcs_api::credentials::Authentication::shared(
                endpoint.as_str(),
                bcs_api::Secret::new(key),
            ),
            target: Some(target),
        });
        let mut recording = Self::cloud(
            RecordingConfig::default(),
            btel_bcs::CloudPublisherConfig::default(),
            delivery,
        );
        if let Destination::Cloud {
            authorization_context,
            ..
        } = &mut recording.destination
        {
            authorization_context.source = source;
        }
        Some(recording)
    }

    /// Deliver through the proposed BCS prepare protocol and presigned PUTs.
    /// No worker starts until engine construction. There is no durable spool.
    pub fn cloud(
        config: RecordingConfig,
        publisher: btel_bcs::CloudPublisherConfig,
        delivery: btel_bcs::delivery::DeliveryConfig,
    ) -> Self {
        let authorization_context = AuthorizationContext {
            endpoint: bcs_api::Endpoint::parse(delivery.prepare_base_url.as_str()).ok(),
            source: CredentialSource::Configured,
            operation: Operation::Ingest,
        };
        Self {
            id: RecordingId::generate(),
            config,
            host: "unknown".to_owned(),
            sources: Vec::new(),
            exit: Arc::default(),
            destination: Destination::Cloud {
                publisher,
                delivery: Box::new(delivery),
                authorization_context,
            },
            initial_failure: None,
            recording_override: RecordingOverride::default(),
        }
    }

    /// Write to `<project-root>/.baml/btel/recordings/<recording-id>`.
    /// Snapshots share `<project-root>/.baml/btel/cas/v4` across recordings.
    /// The caller supplies the resolved BAML project root; the engine does not
    /// rediscover it from the working directory or source paths. Packed binary
    /// hosts without a project root should use [`Self::user_files`].
    /// File output remains explicitly enabled.
    pub fn local_files(project_root: impl AsRef<Path>, config: RecordingConfig) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            host: "unknown".to_owned(),
            sources: Vec::new(),
            exit: Arc::default(),
            destination: Destination::LocalFiles {
                recordings: project_root
                    .as_ref()
                    .join(btel_settings::local_files::RECORDINGS_DIRECTORY),
                cas: project_root
                    .as_ref()
                    .join(btel_settings::local_files::CAS_DIRECTORY),
            },
            initial_failure: None,
            recording_override: RecordingOverride::default(),
        }
    }

    /// Override the root: recordings use `<directory>/<recording-id>` and shared
    /// CAS uses `<directory>/cas/v4`. No I/O or worker
    /// starts until engine construction; `BAML_TELEMETRY=off` ignores this output.
    /// Existing recording directories are never reused. Shutdown awaits writes.
    pub fn local_files_in(directory: impl Into<PathBuf>, config: RecordingConfig) -> Self {
        let recordings = directory.into();
        let cas = recordings.join("cas");
        Self {
            id: RecordingId::generate(),
            config,
            host: "unknown".to_owned(),
            sources: Vec::new(),
            exit: Arc::default(),
            destination: Destination::LocalFiles { recordings, cas },
            initial_failure: None,
            recording_override: RecordingOverride::default(),
        }
    }

    /// Write beneath the current user's home: `~/.baml/btel/recordings`.
    /// Packed programs share `~/.baml/btel/cas/v4` on this machine.
    /// Resolve the home directory only when the enabled engine starts recording.
    /// Fail explicitly if no home directory can be determined; never fall back
    /// to the working directory. Intended for packed programs without a project.
    pub fn user_files(config: RecordingConfig) -> Self {
        Self {
            id: RecordingId::generate(),
            config,
            host: "unknown".to_owned(),
            sources: Vec::new(),
            exit: Arc::default(),
            destination: Destination::UserFiles,
            initial_failure: None,
            recording_override: RecordingOverride::default(),
        }
    }

    pub fn id(&self) -> RecordingId {
        self.id
    }

    #[must_use]
    pub fn with_artifact_telemetry(mut self, policy: ArtifactTelemetry) -> Self {
        self.recording_override = artifact_recovery(&policy);
        self.initial_failure = Some(policy.initial_failure);
        self
    }

    /// What runs this engine: `baml` (the CLI), `lsp`, `pack`, `python`, ...
    #[must_use]
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// The project's BAML sources, `(path, content)`, stored once in the CAS
    /// so a recording can be read against the code that produced it.
    #[must_use]
    pub fn with_sources(mut self, sources: Vec<(String, String)>) -> Self {
        self.sources = sources;
        self
    }

    /// Where the host records how the process ended.
    pub(crate) fn process_exit(&self) -> Arc<btel_types::ProcessExitSlot> {
        Arc::clone(&self.exit)
    }

    fn process(
        &self,
        launch_context: &btel_types::context::Context,
    ) -> btel_recorder::ProcessRecording {
        btel_recorder::ProcessRecording {
            info: btel_types::ProcessInfo {
                process_id: bex_events::ids::ProcessEuid::current().0,
                baml_version: baml_version::CANONICAL_VERSION.to_owned(),
                host: self.host.clone(),
                // Arguments can hold secrets (`--api-key ...`), so a cloud
                // recording does not upload them; local recordings keep them.
                // `args_os`: `args` panics on an argument that is not UTF-8.
                command: if matches!(self.destination, Destination::Cloud { .. }) {
                    Vec::new()
                } else {
                    std::env::args_os()
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect()
                },
                started_at_unix_ns: process_started_at_unix_ns(),
            },
            // Cloud source uploads are a separate feature; local recordings retain their code.
            sources: (matches!(
                self.destination,
                Destination::LocalFiles { .. } | Destination::UserFiles
            ) && !self.sources.is_empty())
            .then(|| {
                let pool = btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default());
                btel_snapshot::string_map(&pool, &self.sources, SOURCES_MAX_BYTES)
            })
            .flatten(),
            context: btel_snapshot::context::capture(
                launch_context,
                &btel_snapshot::SnapshotPool::new(1, btel_snapshot::Limits::default()),
            ),
            exit: Arc::clone(&self.exit),
        }
    }

    pub(crate) fn start(
        self,
        source_snapshot: Option<[u8; 32]>,
        functions: Arc<btel_types::FunctionMetadataTable>,
        launch_context: &btel_types::context::Context,
    ) -> Result<
        (
            Arc<btel_processor::TelemetryRuntime>,
            Option<RecordingDelivery>,
        ),
        EngineError,
    > {
        let transport = btel_settings::transport::ChunkConfig::default();
        let process = self.process(launch_context);
        self.config
            .validate_transport(&transport)
            .map_err(|error| EngineError::Other(error.to_owned()))?;
        let root = match self.destination {
            Destination::InvalidConfiguration(diagnostic) => {
                return Err(EngineError::CloudAuthorization(diagnostic));
            }
            Destination::Cloud {
                publisher,
                delivery,
                authorization_context,
            } => {
                return Self::start_cloud(
                    self.id,
                    self.config,
                    publisher,
                    *delivery,
                    &authorization_context,
                    self.initial_failure.as_ref(),
                    &self.recording_override,
                    source_snapshot,
                    functions,
                    transport,
                    process,
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
        let mut delivery = None;
        let runtime =
            btel_processor::TelemetryRuntime::with_publisher_factory(transport, |control| {
                let failure = control.clone();
                let writer = root.and_then(|(root, cas)| {
                    btel_file::LocalDelivery::create_with_cas(
                        &root,
                        &cas,
                        self.id,
                        btel_file::LocalDeliveryConfig::default(),
                        move |error| {
                            failure.disable(btel_processor::RuntimeError(error.to_string()));
                            tracing::warn!(%error, "telemetry recording disabled");
                        },
                    )
                });
                let builder = RecordingBuilder::new(self.id, self.config)
                    .map_err(std::io::Error::other)?
                    .with_source_snapshot(source_snapshot)
                    .with_function_metadata(functions)
                    .with_process(process);
                let publisher = match writer {
                    Ok(writer) => {
                        let publisher = btel_file::LocalPublisher::new(builder, writer);
                        delivery = publisher.delivery().cloned();
                        publisher
                    }
                    Err(error) => {
                        // Failure to create the directory/file worker also must not
                        // prevent application startup. No automatic retry.
                        tracing::warn!(%error, "telemetry recording startup disabled");
                        control.disable(btel_processor::RuntimeError(format!(
                            "telemetry recording startup: {error}"
                        )));
                        btel_file::LocalPublisher::disabled(builder)
                    }
                };
                Ok(publisher)
            })
            .map_err(|error| EngineError::Other(format!("telemetry processor startup: {error}")))?;
        Ok((runtime, delivery.map(RecordingDelivery::Local)))
    }

    #[expect(clippy::too_many_arguments)]
    fn start_cloud(
        id: RecordingId,
        config: RecordingConfig,
        publisher_config: btel_bcs::CloudPublisherConfig,
        mut delivery_config: btel_bcs::delivery::DeliveryConfig,
        authorization_context: &AuthorizationContext,
        initial_failure: Option<&InitialFailurePolicy>,
        recording_override: &RecordingOverride,
        source_snapshot: Option<[u8; 32]>,
        functions: Arc<btel_types::FunctionMetadataTable>,
        transport: btel_settings::transport::ChunkConfig,
        process: btel_recorder::ProcessRecording,
    ) -> Result<
        (
            Arc<btel_processor::TelemetryRuntime>,
            Option<RecordingDelivery>,
        ),
        EngineError,
    > {
        delivery_config.initial_recording_id = Some(id);
        // Apply the resolved policy to every initial connection failure.
        delivery_config.terminal_initial_connection = true;
        // Caller-configured cloud aborts by default; artifacts supply their own policy.
        let initial_failure = initial_failure
            .cloned()
            .unwrap_or_else(|| InitialFailurePolicy {
                action: InitialFailureAction::Abort,
                ..Default::default()
            });
        let initial_auth_cancel = crate::CancellationToken::new();
        let mut delivery = None;
        let runtime =
            btel_processor::TelemetryRuntime::with_publisher_factory(transport, |control| {
                let failure = control.clone();
                let cancel = initial_auth_cancel.clone();
                let policy = initial_failure.clone();
                let context = authorization_context.clone();
                let warning_override = recording_override.clone();
                let handle = match btel_bcs::delivery::BcsDelivery::new(
                    delivery_config.clone(),
                    move |error| {
                        if matches!(
                            error,
                            btel_bcs::delivery::DeliveryError::InitialAuthorization(_)
                                | btel_bcs::delivery::DeliveryError::InitialConnection(_)
                        ) {
                            match policy.action {
                                InitialFailureAction::Abort => cancel.cancel(),
                                InitialFailureAction::Warn => {
                                    emit_initial_warning(
                                        &policy,
                                        &context,
                                        &error,
                                        &warning_override,
                                    );
                                }
                                InitialFailureAction::Ignore => {}
                            }
                        }
                        failure.disable(btel_processor::RuntimeError(error.to_string()));
                        if !matches!(
                            error,
                            btel_bcs::delivery::DeliveryError::InitialAuthorization(_)
                                | btel_bcs::delivery::DeliveryError::InitialConnection(_)
                        ) {
                            tracing::warn!(%error, "cloud telemetry disabled");
                        }
                    },
                ) {
                    Ok(worker) => {
                        let handle = worker.handle();
                        delivery = Some(RecordingDelivery::Cloud {
                            delivery: Arc::new(worker),
                            initial_auth_cancel: initial_auth_cancel.clone(),
                            authorization_context: authorization_context.clone(),
                            initial_auth_error: Arc::new(OnceLock::new()),
                            initial_failure: initial_failure.clone(),
                            recording_override: recording_override.clone(),
                        });
                        handle
                    }
                    Err(error) => {
                        control.disable(btel_processor::RuntimeError(error.to_string()));
                        btel_bcs::delivery::BcsDeliveryHandle::disabled(error, delivery_config)
                    }
                };
                btel_bcs::CloudPublisher::new(id, config, publisher_config, handle)
                    .map(|publisher| {
                        publisher
                            .with_source_snapshot(source_snapshot)
                            .with_function_metadata(functions)
                            .with_process(process)
                    })
                    .map_err(std::io::Error::other)
            })
            .map_err(|error| EngineError::Other(format!("telemetry processor startup: {error}")))?;
        Ok((runtime, delivery))
    }
}

fn artifact_recovery(policy: &ArtifactTelemetry) -> RecordingOverride {
    let names = match &policy.destination_overrides {
        btel_settings::artifact::DestinationOverrides::Default if policy.embedded.is_none() => {
            Some(("BOUNDARY_PROJECT".to_owned(), "BOUNDARY_API_KEY".to_owned()))
        }
        btel_settings::artifact::DestinationOverrides::Named { project, api_key } => {
            Some((project.clone(), api_key.clone()))
        }
        _ => None,
    };
    RecordingOverride::Artifact {
        level: policy.recording_level_envvar.clone(),
        project: names.as_ref().map(|names| names.0.clone()),
        api_key: names.map(|names| names.1),
    }
}

fn artifact_configuration_error(
    policy: &ArtifactTelemetry,
    endpoint: Option<bcs_api::Endpoint>,
    source: CredentialSource,
    error: bcs_api::Error,
) -> EngineError {
    let diagnostic = AuthorizationContext {
        endpoint,
        source,
        operation: Operation::Ingest,
    }
    .report(error, Outcome::ExecutionCancelled)
    .diagnostic
    .with_recording_override(artifact_recovery(policy));
    EngineError::CloudAuthorization(Box::new(diagnostic))
}

#[allow(clippy::print_stderr)]
fn emit_initial_warning(
    policy: &InitialFailurePolicy,
    context: &AuthorizationContext,
    failure: &btel_bcs::delivery::DeliveryError,
    recording_override: &RecordingOverride,
) {
    let message = policy.warning_message.clone().unwrap_or_else(|| {
        initial_diagnostic(context, failure, Outcome::TelemetryDisabled)
            .with_recording_override(recording_override.clone())
            .to_string()
    });
    eprintln!("warning: {message}");
}

fn initial_diagnostic(
    context: &AuthorizationContext,
    failure: &btel_bcs::delivery::DeliveryError,
    outcome: Outcome,
) -> bcs_api::diagnostics::Diagnostic {
    match failure {
        btel_bcs::delivery::DeliveryError::InitialAuthorization(failure)
        | btel_bcs::delivery::DeliveryError::Api(failure) => {
            context.rejected_failure(failure, outcome)
        }
        btel_bcs::delivery::DeliveryError::InitialConnection(error) => {
            initial_diagnostic(context, error, outcome)
        }
        _ => context.connection_failed(outcome),
    }
}

/// Sources beyond this are cut from the snapshot, which says so.
const SOURCES_MAX_BYTES: usize = 64 << 20;

/// When this process first described itself: close to its start, and the
/// same for every engine it creates.
fn process_started_at_unix_ns() -> i64 {
    static STARTED: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *STARTED.get_or_init(unix_now_ns)
}

fn unix_now_ns() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_nanos()).ok())
        .unwrap_or(0)
}

impl BexEngine {
    /// Whether a panic escaped any root call of this engine.
    pub fn root_panicked(&self) -> bool {
        self.root_panicked
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The process is exiting with `status`: engines shut down after this
    /// end their recordings with it. Hosts call it once, before shutdown.
    pub fn record_process_exit(&self, status: btel_types::ProcessStatus) {
        if let Some(exit) = self
            .telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.process_exit.as_ref())
        {
            exit.set(btel_types::ProcessExit {
                status,
                at_unix_ns: unix_now_ns(),
            });
        }
    }

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
        Self::new_with_config(
            program,
            sys_ops,
            argv,
            crate::EngineConfig {
                runtime_compiler,
                clock_mode,
                recording: Some(recording),
                ..crate::EngineConfig::default()
            },
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

    /// Initial ingestion refusal is fatal to the host execution, unlike later revocation.
    /// Shutdown joins the initial request even when the program produces no recordings.
    pub fn initial_cloud_authorization_error(&self) -> Option<EngineError> {
        self.telemetry
            .as_ref()?
            .delivery
            .as_ref()?
            .initial_auth_error()
    }

    /// The artifact already warned or deliberately ignored an initial failure.
    /// Delivery errors remain inspectable through `telemetry_result`.
    pub fn initial_telemetry_failure_handled(&self) -> bool {
        self.telemetry
            .as_ref()
            .and_then(|telemetry| telemetry.delivery.as_ref())
            .is_some_and(RecordingDelivery::initial_failure_handled)
    }

    pub(crate) fn initial_cloud_auth_cancel(&self) -> Option<&crate::CancellationToken> {
        self.telemetry
            .as_ref()?
            .delivery
            .as_ref()?
            .initial_auth_cancel()
    }

    /// Number of discarded cloud prepare groups or upload targets, not events or
    /// retry attempts. Zero for local recording or when telemetry is off.
    pub fn telemetry_delivery_loss_count(&self) -> u64 {
        match self
            .telemetry
            .as_ref()
            .and_then(|state| state.delivery.as_ref())
        {
            Some(RecordingDelivery::Cloud { delivery, .. }) => delivery.handle().loss_count(),
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
            Some(RecordingDelivery::Cloud { delivery, .. }) => {
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

#[cfg(test)]
mod boundary_env_tests;

#[cfg(test)]
mod source_privacy_tests {
    use super::*;

    #[test]
    fn cloud_recordings_exclude_sources_while_local_recordings_remain_readable() {
        let sources = vec![(
            "main.baml".to_owned(),
            "function Secret() -> int { 42 }".to_owned(),
        )];
        let local =
            TelemetryRecording::local_files("/tmp/baml-source-privacy", RecordingConfig::default())
                .with_sources(sources.clone());
        let cloud = TelemetryRecording::cloud(
            RecordingConfig::default(),
            btel_bcs::CloudPublisherConfig::default(),
            btel_bcs::delivery::DeliveryConfig::new(
                "https://boundary.example.test".parse().unwrap(),
            ),
        )
        .with_sources(sources);
        assert!(
            local
                .process(&btel_types::context::Context::default())
                .sources
                .is_some()
        );
        assert!(
            cloud
                .process(&btel_types::context::Context::default())
                .sources
                .is_none()
        );
    }
}
