use core::time::Duration;
use std::{
    any::type_name,
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::Write,
    sync::Arc,
};

use anyhow::{Context, Result};
use baml_rpc::{
    ast::tops::{FunctionDefinition, SourceCode, AST},
    runtime_api::{
        BlobBatchUploadS3File, BlobMetadataItem, BlobUploadItem, CreateBlobBatchUploadUrl,
        CreateBlobBatchUploadUrlRequest, CreateBlobBatchUploadUrlResponse,
    },
    ApiEndpoint, BamlSrcUploadS3File, CheckBamlSrcUpload, CheckBamlSrcUploadRequest,
    CreateTraceEventUploadUrl, CreateTraceEventUploadUrlRequest, CreateTraceEventUploadUrlResponse,
    NamedType, S3UploadMetadata, TraceEventBatch, TypeDefinition, TypeDefinitionSource,
    TypeReference,
};
use baml_types::{
    tracing::events::{TraceData, TraceEvent},
    BamlValueWithMeta, HasType, TypeIR,
};
use flate2::{write::GzEncoder, Compression};
use futures::StreamExt;
use http::{HeaderMap, HeaderName, HeaderValue};
use once_cell::sync::OnceCell;
use serde::Serialize;
use tokio::sync::mpsc;
#[cfg(not(target_family = "wasm"))]
use tokio::time::*;
use tracing::field;
#[cfg(target_family = "wasm")]
use wasmtimer::tokio::*;

use super::rpc_converters::{
    to_rpc_event, BlobRefCache, BlobStorage, IRRpcState, IntoRpcEvent, TypeLookup,
};
use crate::{runtime::AstSignatureWrapper, tracingv2::storage::interface::TraceEventWithMeta};

enum PublisherMessage {
    Trace(Arc<TraceEventWithMeta>),
    Flush(tokio::sync::oneshot::Sender<()>),
    UpdateRuntime(Arc<RuntimeAST>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

#[derive(Debug)]
pub enum BlobUploaderMessage {
    Upload,
    QueueBlob(super::rpc_converters::blob_storage::BlobWithContent),
    Flush(tokio::sync::oneshot::Sender<()>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// Global publisher channel.
/// When the module is first used, we create a bounded channel and then spawn the publisher task.
/// The channel capacity is limited to 10 batches worth of events to prevent unbounded memory growth.
static PUBLISHING_CHANNEL: OnceCell<mpsc::Sender<PublisherMessage>> = OnceCell::new();
#[cfg(not(target_arch = "wasm32"))]
static PUBLISHING_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();
static BLOB_UPLOADER_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();
static BLOB_UPLOADER_CHANNEL: OnceCell<mpsc::Sender<BlobUploaderMessage>> = OnceCell::new();

fn get_publish_channel(allow_missing: bool) -> Option<&'static mpsc::Sender<PublisherMessage>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(join_handle) = PUBLISHING_TASK.get() else {
            if !allow_missing {
                // baml_log::fatal_once!(
                //     "Tracing publisher not started. Report this bug to the BAML team."
                // );
                // TODO: redo this logic -- we dont start the publisher if there's no api key for example.
            }
            return None;
        };
        if join_handle.is_finished() {
            baml_log::fatal_once!(
                "Tracing publisher ended unexpectedly. Report this bug to the BAML team."
            );
            return None;
        }
    }
    {
        let channel = PUBLISHING_CHANNEL.get();
        channel
    }
}

#[derive(Serialize)]
struct RuntimeAST {
    ast: Arc<AstSignatureWrapper>,
    #[serde(skip)]
    pub client: reqwest::Client,
    #[serde(skip)]
    blob_cache: BlobRefCache,
}

impl RuntimeAST {
    pub fn base_url(&self) -> String {
        // const SAM_API_URL: &str = "https://abe8c5ez29.execute-api.us-east-1.amazonaws.com";
        // const CHRIS_API_URL: &str = "https://o2em3sulde.execute-api.us-east-1.amazonaws.com";
        // return SAM_API_URL.to_string();
        let url = match self.ast.env_var("BOUNDARY_API_URL") {
            Some(url) if !url.is_empty() => url.clone(),
            _ => "https://api.boundaryml.com".to_string(),
        };
        url
    }

    pub fn api_key(&self) -> Option<String> {
        // const CHRIS_API_KEY: &str = "7fc9adc617ed731ba6048daffe0e0de2ec168283624d07a94c2ed520183ea3f722633aa2a5eee9109098254e294f995e";
        // return CHRIS_API_KEY.to_string();
        match self.ast.env_var("BOUNDARY_API_KEY") {
            Some(key) if !key.is_empty() => Some(key.clone()),
            _ => None,
        }
    }

    async fn api_request<'req, 'resp, TEndpoint>(
        &self,
        request: TEndpoint::Request<'req>,
    ) -> Result<TEndpoint::Response<'resp>, ApiError>
    where
        TEndpoint: ApiEndpoint,
    {
        // Wrap the entire request lifecycle in a timeout.
        let timeout_duration = Duration::from_secs(6);

        let fut = async {
            if self.api_key().is_none() {
                return Err(ApiError::Http {
                    status: reqwest::StatusCode::UNAUTHORIZED,
                    body: format!("BOUNDARY_API_KEY is not set for {}", TEndpoint::path()),
                });
            }

            let path = TEndpoint::path();
            // log::info!("api_request request1={path:#?}");

            // A) send the request, propagating low‑level network errors
            let response_builder = self
                .client
                .post(format!("{}{}", self.base_url(), TEndpoint::path()))
                .json(&request)
                .bearer_auth(self.api_key().unwrap());

            let response = response_builder.send().await;
            let path = TEndpoint::path();
            log::debug!("api_request request2={path:#?} response={response:?}");

            let response = match response {
                Ok(response) => response,
                Err(e) => return Err(ApiError::Transport(e)),
            };

            // B) take the status code up‑front
            let status = response.status();

            // We still need the body either way, so pull it into bytes now
            let bytes = response.bytes().await.map_err(ApiError::Transport)?;

            // C) non‑2xx → turn into our own Http error, preserving body for debugging
            if !status.is_success() {
                let body_str = String::from_utf8_lossy(&bytes).to_string();
                return Err(ApiError::Http {
                    status,
                    body: body_str,
                });
            }

            // D) happy path: 2xx → attempt to parse into T
            serde_json::from_slice::<TEndpoint::Response<'resp>>(&bytes)
                .map_err(ApiError::Deserialize)
        };

        match timeout(timeout_duration, fut).await {
            Ok(res) => res,
            Err(_) => Err(ApiError::Timeout(timeout_duration)),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("Transport error: {0}")]
    Transport(reqwest::Error),
    #[error("HTTP error: {status} {body}")]
    Http {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("Failed to deserialize response: {0}")]
    Deserialize(serde_json::Error),
    #[error("Request timed out after {0:?}")]
    Timeout(Duration),
}

impl TypeLookup for RuntimeAST {
    fn type_lookup(&self, name: &str) -> Option<Arc<baml_rpc::BamlTypeId>> {
        self.ast.type_lookup(name)
    }

    fn function_lookup(&self, name: &str) -> Option<Arc<baml_rpc::ast::tops::BamlFunctionId>> {
        self.ast.function_lookup(name)
    }

    fn baml_src_hash(&self) -> Option<String> {
        self.ast.baml_src_hash()
    }
}

impl BlobStorage for RuntimeAST {
    fn blob_cache(&self) -> &BlobRefCache {
        &self.blob_cache
    }
}

pub fn start_publisher(
    lookup: Arc<AstSignatureWrapper>,
    #[cfg(not(target_arch = "wasm32"))] rt: Arc<tokio::runtime::Runtime>,
) {
    if lookup.env_var("BAML_GENERATE").is_some() {
        log::debug!("Skipping publisher because BAML_GENERATE is set");
        return;
    }
    if lookup.env_var("BOUNDARY_API_KEY").is_none() {
        log::debug!("Skipping publisher because BOUNDARY_API_KEY is not set");
        return;
    }
    log::debug!("Starting publisher");

    // Read batch sizes early to calculate channel capacities
    let trace_batch_size = lookup
        .env_var("BAML_TRACE_BATCH_SIZE")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(500);
    let blob_batch_size = lookup
        .env_var("BAML_BLOB_BATCH_SIZE")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);

    // Limit to 10 batches worth of capacity
    let trace_queue_capacity = 4 * trace_batch_size;
    let blob_queue_capacity = 4 * blob_batch_size;

    let mut blob_rx_holder: Option<mpsc::Receiver<BlobUploaderMessage>> = None;
    let blob_tx = match BLOB_UPLOADER_CHANNEL.get() {
        Some(existing) => existing.clone(),
        None => {
            let (new_tx, new_rx) = mpsc::channel::<BlobUploaderMessage>(blob_queue_capacity);
            match BLOB_UPLOADER_CHANNEL.set(new_tx.clone()) {
                Ok(()) => {
                    blob_rx_holder = Some(new_rx);
                    new_tx
                }
                Err(_) => {
                    // Another thread beat us to initialization
                    BLOB_UPLOADER_CHANNEL
                        .get()
                        .expect("blob uploader channel should be initialized")
                        .clone()
                }
            }
        }
    };

    let lookup = Arc::new(RuntimeAST {
        ast: lookup,
        client: reqwest::Client::new(),
        blob_cache: BlobRefCache::with_upload_channel(blob_tx.clone()),
    });

    let channel = if let Some(existing) = PUBLISHING_CHANNEL.get() {
        existing
    } else {
        let Some(blob_rx) = blob_rx_holder.take() else {
            // Another thread is handling initialization; we'll pick up the update next time.
            return;
        };

        #[cfg(not(target_arch = "wasm32"))]
        let rt_clone = rt.clone();

        let lookup_for_publisher = lookup.clone();
        let lookup_for_blob = lookup.clone();
        let blob_tx_for_publisher = blob_tx.clone();

        PUBLISHING_CHANNEL.get_or_init(move || {
            let (tx, rx) = mpsc::channel::<PublisherMessage>(trace_queue_capacity);

            let mut publisher = TracePublisher::new(
                rx,
                lookup_for_publisher,
                blob_tx_for_publisher,
                trace_batch_size,
            );
            let mut blob_uploader = BlobUploader::new(blob_rx, lookup_for_blob, blob_batch_size);

            #[cfg(not(target_arch = "wasm32"))]
            {
                // Spawn the main publisher task
                let handle = rt_clone.spawn(async move { publisher.run().await });
                PUBLISHING_TASK.get_or_init(|| Arc::new(handle));

                // Spawn the blob uploader task
                let blob_handle = rt_clone.spawn(async move { blob_uploader.run().await });
                BLOB_UPLOADER_TASK.get_or_init(|| Arc::new(blob_handle));
            }

            #[cfg(target_arch = "wasm32")]
            {
                wasm_bindgen_futures::spawn_local(async move {
                    publisher.run().await;
                });

                wasm_bindgen_futures::spawn_local(async move {
                    blob_uploader.run().await;
                });
            }

            tx
        })
    };
}

struct TracePublisher {
    batch_size: usize,
    rx: mpsc::Receiver<PublisherMessage>,
    lookup: Arc<RuntimeAST>,
    blob_tx: mpsc::Sender<BlobUploaderMessage>,
}

struct BlobUploader {
    rx: mpsc::Receiver<BlobUploaderMessage>,
    lookup: Arc<RuntimeAST>,
    queued_blobs: Vec<super::rpc_converters::blob_storage::BlobWithContent>,
    batch_size: usize,
}

impl TracePublisher {
    pub fn new(
        rx: mpsc::Receiver<PublisherMessage>,
        lookup: Arc<RuntimeAST>,
        blob_tx: mpsc::Sender<BlobUploaderMessage>,
        batch_size: usize,
    ) -> Self {
        Self {
            rx,
            batch_size,
            lookup,
            blob_tx,
        }
    }

    /// Runs the publisher loop.
    ///
    /// The loop collects incoming events until a batch condition is reached, a timer expires,
    /// or a flush command is received.
    pub async fn run(&mut self) {
        let mut buffer: Vec<Arc<TraceEventWithMeta>> = Vec::new();
        let mut tick_interval = interval(Duration::from_secs(2));

        tracing::debug!(
            message = "Starting publisher loop",
            base_url = self.lookup.base_url(),
        );

        loop {
            tokio::select! {
                // Process any incoming command or event.
                Some(message) = self.rx.recv() => {

                    if self.lookup.api_key().is_none() {
                        tracing::debug!("Skipping trace event because BOUNDARY_API_KEY is not set");
                        continue;
                    }

                    match message {
                        // we expect this to happen first as it sets the 'lookup' object, which is the current runtime for those incoming messages.
                        // All the rest of the messages are guaranteed (99% certainty) to be part of that same
                        // runtime. We can then inject metadata created by the Runtime object into all future messages,
                        // We do this in the into_rpc_event() for example, to create the "RPC" equivalent object, but with some additional metadata.
                        PublisherMessage::UpdateRuntime(lookup) => {
                            self.process_baml_src_upload(&lookup).await;
                            self.lookup = lookup;
                        },
                        PublisherMessage::Trace(event) => {
                            buffer.push(event);
                            if buffer.len() >= self.batch_size {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                                // Trigger blob upload after batch processing (best effort)
                                let _ = self.blob_tx.try_send(BlobUploaderMessage::Upload);
                            }

                        },
                        PublisherMessage::Flush(flush_ack) => {
                            // Flush the current buffer if it has any pending events.
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                            // Flush blob uploader and wait for completion
                            let (blob_ack_tx, blob_ack_rx) = tokio::sync::oneshot::channel();
                            let _ = self.blob_tx.send(BlobUploaderMessage::Flush(blob_ack_tx)).await;
                            let _ = blob_ack_rx.await;
                            // Signal flush completion.
                            let _ = flush_ack.send(());
                            log::debug!("Flush publisher completed")
                        },
                        PublisherMessage::Shutdown(shutdown_ack) => {
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                            // Shutdown blob uploader and wait for completion
                            let (blob_ack_tx, blob_ack_rx) = tokio::sync::oneshot::channel();
                            let _ = self.blob_tx.send(BlobUploaderMessage::Shutdown(blob_ack_tx)).await;
                            let _ = blob_ack_rx.await;
                            let _ = shutdown_ack.send(());
                            break;
                        }
                    }
                }
                // Periodic flush of pending events.
                _ = tick_interval.tick() => {
                    if self.lookup.api_key().is_none() {
                        tracing::debug!("Skipping trace event because BOUNDARY_API_KEY is not set");
                        continue;
                    }
                    if !buffer.is_empty() {
                        self.process_batch(std::mem::take(&mut buffer)).await;
                        // Trigger blob upload after batch processing (best effort)
                        let _ = self.blob_tx.try_send(BlobUploaderMessage::Upload);
                    }
                }
            }
        }
    }

    async fn process_baml_src_upload(&self, lookup: &RuntimeAST) {
        let result = self.process_baml_src_upload_impl(lookup).await;
        if let Err(e) = result {
            tracing::debug!("Failed to upload baml src: {}", e);
        }
    }

    async fn process_baml_src_upload_impl(&self, lookup: &RuntimeAST) -> Result<()> {
        // Convert AstSignatureWrapper to AST
        let ast = &lookup.ast;

        // Convert functions
        let functions: Vec<FunctionDefinition> = ast
            .functions
            .iter()
            .map(|(name, signature)| {
                let inputs = signature
                    .inputs
                    .iter()
                    .map(|(name, field_type)| NamedType {
                        name: name.clone(),
                        type_ref: field_type.to_rpc_event(self.lookup.as_ref()),
                    })
                    .collect();

                FunctionDefinition {
                    function_id: signature.function_id.0.clone(),
                    inputs,
                    output: signature.output.to_rpc_event(self.lookup.as_ref()),
                    dependencies: signature
                        .function_id
                        .1
                        .iter()
                        .map(|dep| dep.0.clone())
                        .collect(),
                }
            })
            .collect();

        // Convert types
        let types: Vec<TypeDefinition> =
            ast.types
                .values()
                .map(|type_with_deps| {
                    let type_id_arc = &type_with_deps.type_id.0;
                    let dependencies_arc = &type_with_deps.type_id.1;

                    let concrete_type_id = (**type_id_arc).clone();
                    let concrete_dependencies = (**dependencies_arc).clone();

                    let node_id = &concrete_type_id.0;
                    let type_name_str = node_id.type_name();

                    match type_name_str {
                        "class" => {
                            let fields: Vec<NamedType> = type_with_deps
                                .class_fields
                                .as_ref()
                                .map_or(vec![], |arc_fields| {
                                    (**arc_fields)
                                        .iter()
                                        .map(|(name, field_type_arc)| NamedType {
                                            name: name.clone(),
                                            type_ref: (**field_type_arc)
                                                .to_rpc_event(self.lookup.as_ref()),
                                        })
                                        .collect()
                                });
                            TypeDefinition::Class {
                                type_id: concrete_type_id,
                                fields,
                                source: TypeDefinitionSource::CompileTime,
                                dependencies: concrete_dependencies
                                    .iter()
                                    .map(|d| d.0.clone())
                                    .collect(),
                            }
                        }
                        "enum" => {
                            let values: Vec<String> = type_with_deps
                                .enum_values
                                .as_ref()
                                .map_or(vec![], |arc_values| (**arc_values).clone());
                            TypeDefinition::Enum {
                                type_id: concrete_type_id,
                                values,
                                source: TypeDefinitionSource::CompileTime,
                                dependencies: concrete_dependencies
                                    .iter()
                                    .map(|d| d.0.clone())
                                    .collect(),
                            }
                        }
                        "type_alias" => TypeDefinition::Alias {
                            type_id: concrete_type_id,
                            rhs: (*type_with_deps.field_type).to_rpc_event(self.lookup.as_ref()),
                        },
                        _ => TypeDefinition::Alias {
                            type_id: concrete_type_id,
                            rhs: TypeReference::string(),
                        },
                    }
                })
                .collect();

        // Convert source_code
        let source_code: Vec<SourceCode> = ast
            .source_code
            .iter()
            .map(|(path, content)| {
                let mut hasher: DefaultHasher = DefaultHasher::new();
                content.hash(&mut hasher);
                let content_hash = format!("{:x}", hasher.finish());
                SourceCode {
                    file_name: path.to_string_lossy().to_string(),
                    content: content.to_string(),
                    content_hash,
                }
            })
            .collect();

        let ast_obj = std::sync::Arc::new(AST {
            functions,
            types,
            // TODO: optimize this by not cloning the source code
            source_code: source_code.clone(),
        });

        // Calculate hash of the entire BAML source
        let baml_src_hash = ast.baml_src_hash().unwrap_or_default();

        tracing::debug!(
            "Checking if BAML source upload is needed (hash: {})",
            baml_src_hash
        );

        // Check if we should upload
        let check_response = match lookup
            .api_request::<CheckBamlSrcUpload>(CheckBamlSrcUploadRequest {
                baml_src_hash,
                baml_runtime: Some(env!("CARGO_PKG_VERSION").to_string()),
            })
            .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!("Failed to check BAML source upload status: {}", e);
                return Err(e.into());
            }
        };
        tracing::debug!("check_response={:?}", check_response);

        if !check_response.should_upload {
            tracing::debug!("BAML source already uploaded, skipping");
            return Ok(());
        }

        tracing::debug!("Uploading BAML source");

        let upload_url = check_response
            .upload_url
            .ok_or_else(|| anyhow::anyhow!("No upload URL provided when should_upload is true"))?;

        let upload_metadata = check_response.upload_metadata.ok_or_else(|| {
            anyhow::anyhow!("No upload metadata provided when should_upload is true")
        })?;

        // Create the upload payload
        let payload = BamlSrcUploadS3File { ast: ast_obj };

        // Upload to S3
        lookup
            .client
            .put(upload_url)
            .json(&payload)
            .headers({
                let mut headers = reqwest::header::HeaderMap::new();
                for (key, value) in upload_metadata.to_map() {
                    let header_name = format!("x-amz-meta-{key}");
                    if let (Ok(name), Ok(val)) = (
                        reqwest::header::HeaderName::from_bytes(header_name.as_bytes()),
                        reqwest::header::HeaderValue::from_str(&value),
                    ) {
                        headers.insert(name, val);
                    }
                }
                headers
            })
            .send()
            .await
            .context("Failed to upload BAML source to S3")?;

        tracing::debug!("Successfully uploaded BAML source");
        Ok(())
    }

    async fn process_batch(&self, batch: Vec<Arc<TraceEventWithMeta>>) {
        let batch_result = self.process_batch_impl(batch).await;
        if let Err(e) = batch_result {
            baml_log::debug!("Failed to upload trace events: {:?}", e);
        }
    }

    // Remove the process_blob_uploads method as it's now handled by BlobUploader

    /// Process a batch of events.
    ///
    /// This method:
    ///   1. Converts events to RPC format with blob extraction.
    ///   2. Serializes the events into JSON.
    ///   3. Checks the payload size against a configurable limit.
    ///   4. Uploads the JSON to S3 via presigned URL.
    async fn process_batch_impl(&self, batch: Vec<Arc<TraceEventWithMeta>>) -> Result<()> {
        // log::info!("Processing {:#?}", batch);
        // Assemble the upload request structure.
        let trace_event_batch = TraceEventBatch {
            events: batch
                .iter()
                .map(|e| to_rpc_event(e, self.lookup.as_ref()))
                .collect(),
        };

        // log::info!("trace_event_batch={:#?}", trace_event_batch);

        // tracing::info!(
        //     message = "Trying to upload trace events",
        //     batch_size = batch.len()
        // );

        // Serialize to bytes once
        let payload_bytes = serde_json::to_vec(&trace_event_batch)
            .context("Failed to serialize trace event batch")?;

        let uncompressed_size_mb = payload_bytes.len() as f64 / 1_048_576.0;

        // Compress if payload is larger than threshold (default 2 MB)
        let compression_threshold_mb = self
            .lookup
            .ast
            .env_var("BAML_TRACE_COMPRESSION_THRESHOLD_MB")
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(2.0);

        let (final_payload, content_encoding) = if uncompressed_size_mb > compression_threshold_mb {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(&payload_bytes)
                .context("Failed to write to gzip encoder")?;
            let compressed = encoder
                .finish()
                .context("Failed to finish gzip compression")?;

            let compressed_size_bytes = compressed.len() as f64;
            let compressed_size_mb = compressed_size_bytes / 1_048_576.0;
            log::debug!(
                "Compressed trace batch from {:.5} MB to {:.2} MB ({:.1}% reduction, {} events)",
                uncompressed_size_mb,
                compressed_size_mb,
                (1.0 - compressed_size_mb / uncompressed_size_mb) * 100.0,
                batch.len()
            );

            (compressed, Some("gzip"))
        } else {
            (payload_bytes, None)
        };

        // Check size limit after compression
        let max_upload_mb = self
            .lookup
            .ast
            .env_var("BAML_MAX_TRACE_UPLOAD_MB")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10);

        let final_size_mb = final_payload.len() as f64 / 1_048_576.0;

        if final_size_mb > max_upload_mb as f64 {
            baml_log::warn!(
                "Skipping Boundary trace batch upload: payload size {:.2} MB exceeds limit of {} MB ({} events).",
                final_size_mb,
                max_upload_mb,
                batch.len()
            );
            return Ok(());
        }

        // Optionally write to file for testing
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(trace_file_path) = std::env::var("BAML_TRACE_FILE") {
                eprintln!("Writing trace events to file: {trace_file_path}");
                use tokio::fs::OpenOptions;
                if let Ok(mut file) = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&trace_file_path)
                    .await
                {
                    for e in trace_event_batch.events.iter() {
                        if let Ok(json) = serde_json::to_string(e) {
                            use tokio::io::AsyncWriteExt;
                            if let Err(e) = file.write_all(format!("{json}\n").as_bytes()).await {
                                log::error!("Failed to write to trace file: {e}");
                            }
                        }
                    }
                }
            }
        }

        // Upload via HTTP with retry logic.
        // TODO watch out with time crate

        let upload_url_details = match self
            .lookup
            .api_request::<CreateTraceEventUploadUrl>(CreateTraceEventUploadUrlRequest {
                baml_runtime: Some(env!("CARGO_PKG_VERSION").to_string()),
            })
            .await
        {
            Ok(response) => response,
            Err(e) => {
                log::debug!("Failed to upload trace events: {e}");
                return Err(e.into());
            }
        };

        let mut request = self
            .lookup
            .client
            .put(upload_url_details.upload_url)
            .header("Content-Type", "application/json")
            .body(final_payload);

        // Add Content-Encoding header if compressed
        if let Some(encoding) = content_encoding {
            request = request.header("Content-Encoding", encoding);
        }

        request
            .headers(
                upload_url_details
                    .upload_metadata
                    // S3 upload URL shoves the project_id into S3ObjectMetadata
                    // When we process the S3 Upload notification, the Queue processor
                    // relies on this metadata to determine the project_id.
                    .as_reqwest_headers()
                    .context(format!(
                        "Failed to convert {} to HeaderMap",
                        type_name::<S3UploadMetadata>(),
                    ))?,
            )
            .send()
            .await
            .context("Failed to upload trace events to S3")?;

        log::debug!(
            "Successfully uploaded batch of {} events ({:.2} MB)",
            batch.len(),
            final_size_mb
        );
        Ok(())
    }
}

trait AsReqwestHeaders {
    fn as_reqwest_headers(&self) -> Result<HeaderMap>;
}

impl BlobUploader {
    pub fn new(
        rx: mpsc::Receiver<BlobUploaderMessage>,
        lookup: Arc<RuntimeAST>,
        batch_size: usize,
    ) -> Self {
        Self {
            rx,
            lookup,
            queued_blobs: Vec::new(),
            batch_size,
        }
    }

    pub async fn run(&mut self) {
        let mut upload_interval = interval(Duration::from_secs(2));
        upload_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                Some(message) = self.rx.recv() => {
                    // log::info!("Blob uploader received message: ", message);
                    if self.lookup.api_key().is_none() {
                        log::debug!("Skipping blob upload because BOUNDARY_API_KEY is not set");
                        continue;
                    }

                    match message {
                        BlobUploaderMessage::Upload => {
                            // No-op in new architecture - blobs are queued immediately when stored
                        },
                        BlobUploaderMessage::QueueBlob(blob) => {
                            self.queued_blobs.push(blob);

                            // If we've reached the batch size, upload immediately
                            if self.queued_blobs.len() >= self.batch_size {
                                self.process_queued_blobs().await;
                            }
                        },
                        BlobUploaderMessage::Flush(flush_ack) => {
                            log::debug!("Flush blob uploader started");
                            self.process_queued_blobs().await;
                            let _ = flush_ack.send(());
                        },
                        BlobUploaderMessage::Shutdown(shutdown_ack) => {
                            self.process_queued_blobs().await;
                            let _ = shutdown_ack.send(());
                            break;
                        }
                    }
                }
                _ = upload_interval.tick() => {
                    if self.lookup.api_key().is_none() {
                        log::debug!("Skipping blob upload because BOUNDARY_API_KEY is not set");
                        continue;
                    }
                    if !self.queued_blobs.is_empty() {
                        self.process_queued_blobs().await;
                    }
                }
            }
        }
    }

    async fn process_queued_blobs(&mut self) {
        if self.queued_blobs.is_empty() {
            return;
        }

        let queued_len = self.queued_blobs.len();
        log::debug!("Processing {queued_len} queued blobs");
        let blobs_to_upload = std::mem::take(&mut self.queued_blobs);

        match self.upload_blob_batch(blobs_to_upload).await {
            Ok(()) => {
                log::debug!("Successfully uploaded batch of {queued_len} blobs");
            }
            Err(e) => {
                log::error!("Failed to upload queued blob batch ({queued_len} blobs): {e}");
            }
        }
    }

    async fn upload_blob_batch(
        &self,
        blobs: Vec<super::rpc_converters::blob_storage::BlobWithContent>,
    ) -> Result<()> {
        if blobs.is_empty() {
            return Ok(());
        }

        // Prepare metadata for the API request
        let blob_metadata: Vec<BlobMetadataItem> = blobs
            .iter()
            .map(|blob| BlobMetadataItem {
                blob_hash: blob.metadata.blob_hash.clone(),
                function_call_id: blob.metadata.function_call_id.clone(),
                media_type: blob.metadata.media_type.clone(),
                size_bytes: blob.metadata.size_bytes,
            })
            .collect();

        // Get upload URL and check which blobs already exist
        let blob_endpoint = format!(
            "{}{}",
            self.lookup.base_url(),
            <CreateBlobBatchUploadUrl as ApiEndpoint>::path()
        );
        log::debug!(
            "Requesting blob upload URL for {} blobs at {}",
            blob_metadata.len(),
            blob_endpoint
        );

        let upload_response = match self
            .lookup
            .api_request::<CreateBlobBatchUploadUrl>(CreateBlobBatchUploadUrlRequest {
                blob_metadata,
                baml_runtime: Some(env!("CARGO_PKG_VERSION").to_string()),
            })
            .await
        {
            Ok(response) => response,
            Err(e) => {
                log::error!("Failed to get blob upload URL: {e}");
                return Err(e.into());
            }
        };
        if let Ok(parsed_url) = reqwest::Url::parse(&upload_response.s3_presigned_url) {
            log::debug!(
                "Received blob upload URL host={} path={} ({} blobs excluded)",
                parsed_url.host_str().unwrap_or_default(),
                parsed_url.path(),
                upload_response.exclude_blobs.len()
            );
        } else {
            log::debug!(
                "Received blob upload URL ({} blobs excluded)",
                upload_response.exclude_blobs.len()
            );
        }

        // Filter out blobs that already exist
        let blobs_to_upload: Vec<_> = blobs
            .into_iter()
            .filter(|blob| {
                !upload_response
                    .exclude_blobs
                    .contains(&blob.metadata.blob_hash)
            })
            .collect();

        if blobs_to_upload.is_empty() {
            log::debug!("All blobs already exist, skipping upload");
            return Ok(());
        }

        // Prepare the upload payload
        let upload_items: Vec<BlobUploadItem> = blobs_to_upload
            .iter()
            .map(|blob| BlobUploadItem {
                function_call_id: blob.metadata.function_call_id.clone(),
                blob_hash: blob.metadata.blob_hash.clone(),
                // Content here is the base64 string bytes; convert to String
                base64_payload: String::from_utf8_lossy(&blob.content).to_string(),
                media_type: blob.metadata.media_type.clone(),
            })
            .collect();

        let batch_file = BlobBatchUploadS3File {
            blobs: upload_items,
        };

        // Measure payload size (bytes) for throughput logging
        let payload_bytes = serde_json::to_string(&batch_file)
            .context("Failed to serialize blob batch for size measurement")?
            .into_bytes()
            .len();

        // Upload to S3 and measure elapsed time
        let start_time = std::time::Instant::now();
        let upload_result = self
            .lookup
            .client
            .put(&upload_response.s3_presigned_url)
            .json(&batch_file)
            .headers(
                upload_response
                    .upload_metadata
                    .as_reqwest_headers()
                    .context("Failed to convert upload metadata to headers")?,
            )
            .send()
            .await;
        let elapsed = start_time.elapsed();

        match upload_result {
            Ok(response) => {
                let secs = elapsed.as_secs_f64().max(1e-9);
                let kb = payload_bytes as f64 / 1024.0;
                let kbps = kb / secs;
                log::debug!(
                    "Blob batch upload completed with status {} ({} blobs, {:.2} kB in {:.2}s, {:.2} kB/s)",
                    response.status(),
                    blobs_to_upload.len(),
                    kb,
                    secs,
                    kbps
                );
            }
            Err(e) => {
                let secs = elapsed.as_secs_f64().max(1e-9);
                let kb = payload_bytes as f64 / 1024.0;
                let kbps = kb / secs;
                log::error!(
                    "Failed to upload BAML blob batch to S3 after {:.2}s ({} blobs, attempted {:.2} kB, {:.2} kB/s): {e}",
                    secs,
                    blobs_to_upload.len(),
                    kb,
                    kbps
                );
                return Err(e.into());
            }
        }

        Ok(())
    }
}

impl AsReqwestHeaders for S3UploadMetadata {
    fn as_reqwest_headers(&self) -> Result<HeaderMap> {
        let as_map = serde_json::to_value(self).expect("Failed to serialize S3UploadMetadata");
        as_map
            .as_object()
            .expect("Failed to convert S3UploadMetadata to object")
            .iter()
            .map(|(k, v)| {
                Ok((
                    HeaderName::from_bytes(format!("x-amz-meta-{k}").as_bytes())?,
                    HeaderValue::from_str(v.as_str().unwrap())?,
                ))
            })
            .collect::<Result<HeaderMap>>()
    }
}

async fn flush_blob_uploader_channel(timeout_duration: Duration) -> anyhow::Result<()> {
    let Some(blob_tx) = BLOB_UPLOADER_CHANNEL.get() else {
        return Ok(());
    };

    let (blob_ack_tx, blob_ack_rx) = tokio::sync::oneshot::channel();
    blob_tx
        .send(BlobUploaderMessage::Flush(blob_ack_tx))
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    match timeout(timeout_duration, blob_ack_rx).await {
        Ok(Ok(())) => {
            log::debug!("Flush blob uploader completed");
            Ok(())
        }
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(anyhow::anyhow!(
            "Blob flush timed out after {:?}",
            timeout_duration
        )),
    }
}

pub fn publish_trace_event(event: Arc<TraceEventWithMeta>) -> anyhow::Result<()> {
    let Some(channel) = get_publish_channel(false) else {
        return Ok(());
    };
    match channel.try_send(PublisherMessage::Trace(event)) {
        Ok(()) => Ok(()),
        Err(mpsc::error::TrySendError::Full(_)) => {
            log::warn!(
                "Trace event queue is full (max 4 batches). Dropping trace event. \
                Consider increasing BAML_TRACE_BATCH_SIZE or reducing trace volume."
            );
            Ok(())
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            Err(anyhow::anyhow!("Trace publisher channel is closed"))
        }
    }
}

// Note, the library we are using doesnt seem to work well for flushing in Node
// but that's ok since noone uses our wasm build in node for logging.
// https://github.com/whizsid/wasmtimer-rs/issues/26
pub async fn flush() -> anyhow::Result<()> {
    log::debug!("Flushing traces [rust]");
    // Set a timeout to avoid waiting indefinitely.
    let timeout_duration = Duration::from_secs(30);

    // First try to flush the trace publisher (which should also flush blobs internally)
    let mut publisher_result: Option<anyhow::Result<()>> = None;
    if let Some(channel) = get_publish_channel(false) {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let send_res = channel
            .send(PublisherMessage::Flush(ack_tx))
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()));
        if let Err(e) = send_res {
            publisher_result = Some(Err(e));
        } else {
            publisher_result = Some(match timeout(timeout_duration, ack_rx).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.into()),
                Err(_) => Err(anyhow::anyhow!(
                    "Flush timed out after {:?}",
                    timeout_duration
                )),
            });
        }
    } else {
        log::debug!("No publish channel found [rust]");
    }

    // Always flush the blob uploader explicitly as well to guarantee no leftovers
    log::debug!("Flushing blob uploader [rust]");
    let blob_result = flush_blob_uploader_channel(timeout_duration).await;
    log::debug!("Flushing blob uploader [rust] completed");
    // Prefer reporting blob uploader errors if any; otherwise propagate publisher errors
    blob_result?;

    if let Some(Err(e)) = publisher_result {
        return Err(e);
    }
    Ok(())
}
