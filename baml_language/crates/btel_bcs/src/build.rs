//! Registration of builds with BCS (BEP-088): `POST /v1/builds` with the
//! build, its package and program, then a PUT of the source to the URL the
//! answer presigns. Nothing waits on it, and a build that fails to register
//! loses only its row: a recording names builds by ID alone.

use std::time::Duration;

use bcs_api::wire::builds as wire;
use btel_types::BuildRegistration;
use bytes::Bytes;
use reqwest::Client;
use sha2::{Digest, Sha256};

use crate::{
    delivery::{DeliveryConfig, DeliveryError, now_ms, read_response, retryable},
    plan::checked_url,
};

/// The longest source BCS takes: one blob of its upload envelope.
const MAX_SOURCE_BYTES: usize = 64 * 1024 * 1024;

/// One build as it is sent: the request, and the source its descriptor
/// names.
#[derive(Clone, Debug)]
pub struct Registration {
    request: wire::RegisterBuild,
    source: Option<Bytes>,
}

impl Registration {
    /// A source BCS would refuse (a file or the whole past its limits, or a
    /// path it does not take) is left out, and the build registers without
    /// it.
    #[must_use]
    pub fn new(registration: &BuildRegistration) -> Self {
        let build = &registration.build;
        let package = &registration.package;
        let source = registration
            .source
            .as_ref()
            .filter(|files| files.iter().all(|(path, text)| uploadable(path, text)))
            .and_then(|files| serde_json::to_vec(files).ok())
            .filter(|bytes| !bytes.is_empty() && bytes.len() <= MAX_SOURCE_BYTES)
            .map(Bytes::from);
        Self {
            request: wire::RegisterBuild {
                build: wire::Build {
                    build_id: hex::encode(build.build_id),
                    timestamp_ns: build.timestamp_unix_ns.to_string(),
                    origin: build.origin.as_str().to_owned(),
                    git: build.git.as_ref().map(|git| wire::Git {
                        remote: git.remote.clone(),
                        hash: hex::encode(git.hash.as_bytes()),
                        dirty: git.dirty,
                        branch: git.branch.clone(),
                        package_path: git.package_path.clone(),
                    }),
                    source_hash: hex::encode(build.source_hash),
                    compilation_unit_hash: hex::encode(build.compilation_unit_hash),
                },
                package: wire::Package {
                    compilation_unit_hash: hex::encode(package.compilation_unit_hash),
                    package_interface: hex::encode(package.package_interface),
                    dependencies: package.dependencies.iter().map(hex::encode).collect(),
                    name: package.name.clone(),
                    version: package.version.clone(),
                    compiler_version: package.compiler_version.clone(),
                },
                program: registration.program.as_ref().map(|program| wire::Program {
                    program_id: hex::encode(program.program_id),
                    static_packages: program.static_packages.iter().map(hex::encode).collect(),
                }),
                source: source.as_ref().map(|bytes| wire::Source {
                    sha256: hex::encode(Sha256::digest(bytes)),
                    byte_length: bytes.len() as u64,
                }),
            },
            source,
        }
    }

    /// Registers the build, then uploads its source when BCS asks for it.
    pub(crate) async fn send(
        &self,
        client: &Client,
        config: &DeliveryConfig,
    ) -> Result<(), DeliveryError> {
        let upload = self.register(client, config).await?;
        let (
            wire::SourceUpload::UploadRequired {
                presigned_put_url,
                expires_at_unix_ms,
            },
            Some(source),
        ) = (upload, &self.source)
        else {
            return Ok(());
        };
        let url = checked_url(&presigned_put_url, &config.prepare_base_url)?;
        put(client, config, url.as_str(), source, expires_at_unix_ms).await
    }

    async fn register(
        &self,
        client: &Client,
        config: &DeliveryConfig,
    ) -> Result<wire::SourceUpload, DeliveryError> {
        let mut url = config.prepare_base_url.clone();
        url.path_segments_mut()
            .map_err(|()| DeliveryError::InvalidConfig)?
            .pop_if_empty()
            .extend(["v1", "builds"]);
        let body = Bytes::from(
            serde_json::to_vec(&bcs_api::credentials::Targeted {
                target: config
                    .authorization
                    .as_ref()
                    .and_then(|authorization| authorization.target.as_ref()),
                payload: &self.request,
            })
            .map_err(|_| DeliveryError::Encoding)?,
        );
        let mut last_failure = None;
        for attempt in 0..config.max_attempts {
            match bcs_api::telemetry::register_build(
                client,
                &url,
                config.bearer_token.as_deref(),
                config.authorization.as_ref(),
                body.clone(),
            )
            .await
            {
                Ok(response) => {
                    let bytes = read_response(response, config.max_response_bytes).await?;
                    return serde_json::from_slice(&bytes).map_err(|_| DeliveryError::InvalidPlan);
                }
                Err(error)
                    if error
                        .http_failure()
                        .is_some_and(|failure| !retryable(failure.status)) =>
                {
                    return Err(error
                        .http_failure()
                        .cloned()
                        .map_or(DeliveryError::Http, DeliveryError::Api));
                }
                Err(error) => last_failure = error.http_failure().cloned(),
            }
            if attempt + 1 < config.max_attempts {
                tokio::time::sleep(config.retry_delay).await;
            }
        }
        Err(last_failure.map_or(DeliveryError::Http, DeliveryError::Api))
    }

    /// The longest [`Self::send`] takes: every attempt of both requests at
    /// its full timeout.
    fn window(&self, config: &DeliveryConfig) -> Duration {
        let source = self.source.as_ref().map_or(0, Bytes::len);
        config
            .put_window(0)
            .zip(config.put_window(source))
            .and_then(|(register, put)| register.checked_add(put))
            .unwrap_or(Duration::MAX)
    }
}

/// Whether BCS takes `text` at `path`: it refuses a path that is not a plain
/// relative one, and a path or file longer than one captured leaf.
fn uploadable(path: &str, text: &str) -> bool {
    let limit = btel_settings::snapshot::MAX_LEAF_BYTES;
    path.len() <= limit
        && text.len() <= limit
        && !path.contains('\\')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

async fn put(
    client: &Client,
    config: &DeliveryConfig,
    url: &str,
    source: &Bytes,
    expiry: u64,
) -> Result<(), DeliveryError> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    for attempt in 0..config.max_attempts {
        let remaining = expiry
            .checked_sub(now_ms()?)
            .filter(|&ms| ms > 0)
            .ok_or(DeliveryError::Expired)?;
        let answer = bcs_api::telemetry::upload(
            client,
            url,
            headers.clone(),
            reqwest::Body::from(source.clone()),
            source.len(),
            config
                .put_timeout(source.len())
                .min(Duration::from_millis(remaining)),
        )
        .await;
        match answer {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) if !retryable(response.status()) => return Err(DeliveryError::Http),
            _ => {}
        }
        if attempt + 1 < config.max_attempts {
            tokio::time::sleep(config.retry_delay).await;
        }
    }
    Err(DeliveryError::Http)
}

/// Registers `registration` and uploads its source, on the calling thread:
/// for a build that runs no program (`baml pack`, `baml generate`).
///
/// # Errors
///
/// The configuration is invalid, or BCS did not take the build.
pub fn register_blocking(
    config: &DeliveryConfig,
    registration: &BuildRegistration,
) -> Result<(), DeliveryError> {
    config.validate()?;
    let registration = Registration::new(registration);
    let (runtime, client) = crate::delivery::runtime_and_client(config)?;
    let sent = runtime.block_on(async {
        tokio::time::timeout(
            registration.window(config),
            registration.send(&client, config),
        )
        .await
        .unwrap_or(Err(DeliveryError::ShutdownTimeout))
    });
    // As the delivery worker: a hung DNS lookup must not hold the caller.
    runtime.shutdown_background();
    sent
}
