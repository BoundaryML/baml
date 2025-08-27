use core::time::Duration;
use std::{
    any::type_name,
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
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
use crate::{
    runtime::{AstSignatureWrapper, InternalBamlRuntime},
    tracingv2::storage::interface::TraceEventWithMeta,
};

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
/// When the module is first used, we create an unbounded channel and then spawn the publisher task.
static PUBLISHING_CHANNEL: OnceCell<mpsc::UnboundedSender<PublisherMessage>> = OnceCell::new();
#[cfg(not(target_arch = "wasm32"))]
static PUBLISHING_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();
static BLOB_UPLOADER_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();

fn get_publish_channel(
    allow_missing: bool,
) -> Option<&'static mpsc::UnboundedSender<PublisherMessage>> {
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
        if self.api_key().is_none() {
            return Err(ApiError::Http {
                status: reqwest::StatusCode::UNAUTHORIZED,
                body: format!("BOUNDARY_API_KEY is not set for {}", TEndpoint::path()),
            });
        }
        // A) send the request, propagating low‑level network errors
        let response = self
            .client
            .post(format!("{}{}", self.base_url(), TEndpoint::path()))
            .json(&request)
            .bearer_auth(self.api_key().unwrap());
        let response = response.send().await;

        let response = match response {
            Ok(response) => response,
            Err(e) => {
                return Err(ApiError::Transport(e));
            }
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
        serde_json::from_slice::<TEndpoint::Response<'resp>>(&bytes).map_err(ApiError::Deserialize)
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

    // Create the blob upload channel first
    let (blob_tx, blob_rx) = mpsc::unbounded_channel::<BlobUploaderMessage>();

    let lookup = Arc::new(RuntimeAST {
        ast: lookup,
        client: reqwest::Client::new(),
        blob_cache: BlobRefCache::with_upload_channel(blob_tx.clone()),
    });

    // Use get_or_init to ensure thread-safe initialization
    let channel = PUBLISHING_CHANNEL.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel::<PublisherMessage>();

        let mut publisher = TracePublisher::new(rx, lookup.clone(), blob_tx.clone());
        let mut blob_uploader = BlobUploader::new(blob_rx, lookup.clone());

        #[cfg(not(target_arch = "wasm32"))]
        {
            // Spawn the main publisher task
            let handle = rt.spawn(async move { publisher.run().await });
            PUBLISHING_TASK.get_or_init(|| Arc::new(handle));

            // Spawn the blob uploader task
            let blob_handle = rt.spawn(async move { blob_uploader.run().await });
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
    });

    let _ = channel.send(PublisherMessage::UpdateRuntime(lookup));
}

/// Gracefully shutdown the TracePublisher.
/// 1. Sends a Shutdown message and waits for its ack.
/// 2. Awaits the background task's JoinHandle so Drop runs.
pub async fn shutdown_publisher() -> anyhow::Result<()> {
    log::debug!("Shutting down publisher");
    // 1. send Shutdown
    let Some(channel) = get_publish_channel(true) else {
        return Ok(());
    };
    let Some(blob_channel) = BLOB_UPLOADER_TASK.get() else {
        return Ok(());
    };
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    channel
        .send(PublisherMessage::Shutdown(ack_tx))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // 2. wait for the ack (so we flush remaining events)
    ack_rx
        .await
        .map_err(|e| anyhow::anyhow!("shutdown ack failed: {}", e))?;

    Ok(())
}

struct TracePublisher {
    batch_size: usize,
    rx: mpsc::UnboundedReceiver<PublisherMessage>,
    lookup: Arc<RuntimeAST>,
    blob_tx: mpsc::UnboundedSender<BlobUploaderMessage>,
}

struct BlobUploader {
    rx: mpsc::UnboundedReceiver<BlobUploaderMessage>,
    lookup: Arc<RuntimeAST>,
    queued_blobs: Vec<super::rpc_converters::blob_storage::BlobWithContent>,
    batch_size: usize,
}

impl TracePublisher {
    pub fn new(
        rx: mpsc::UnboundedReceiver<PublisherMessage>,
        lookup: Arc<RuntimeAST>,
        blob_tx: mpsc::UnboundedSender<BlobUploaderMessage>,
    ) -> Self {
        let batch_size = lookup
            .ast
            .env_var("BAML_TRACE_BATCH_SIZE")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(500);

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
                                // Trigger blob upload after batch processing
                                let _ = self.blob_tx.send(BlobUploaderMessage::Upload);
                            }

                        },
                        PublisherMessage::Flush(flush_ack) => {
                            // Flush the current buffer if it has any pending events.
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                            // Flush blob uploader and wait for completion
                            let (blob_ack_tx, blob_ack_rx) = tokio::sync::oneshot::channel();
                            let _ = self.blob_tx.send(BlobUploaderMessage::Flush(blob_ack_tx));
                            let _ = blob_ack_rx.await;
                            // Signal flush completion.
                            let _ = flush_ack.send(());
                        },
                        PublisherMessage::Shutdown(shutdown_ack) => {
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                            // Shutdown blob uploader and wait for completion
                            let (blob_ack_tx, blob_ack_rx) = tokio::sync::oneshot::channel();
                            let _ = self.blob_tx.send(BlobUploaderMessage::Shutdown(blob_ack_tx));
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
                        // Trigger blob upload after batch processing
                        let _ = self.blob_tx.send(BlobUploaderMessage::Upload);
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
                tracing::error!("Failed to check BAML source upload status: {}", e);
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
    ///   3. Uploads the JSON to S3 via presigned URL.
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

        // Serialize to JSON.
        // #[cfg(not(target_arch = "wasm32"))]
        // {
        //     use tokio::fs::OpenOptions;
        //     if let Ok(mut file) = OpenOptions::new()
        //         .create(true)
        //         .append(true)
        //         .open("/tmp/trace_events.json")
        //         .await
        //     {
        //         for e in trace_event_batch.events.iter() {
        //             if let Ok(json) = serde_json::to_string(e) {
        //                 use tokio::io::AsyncWriteExt;
        //                 if let Err(e) = file.write_all(format!("{}\n", json).as_bytes()).await {
        //                     log::error!("Failed to write to trace file: {}", e);
        //                 }
        //             }
        //         }
        //     }
        // }

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

        self.lookup
            .client
            .put(upload_url_details.upload_url)
            .json(&trace_event_batch)
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

        log::debug!("Successfully uploaded batch of {} events", batch.len());
        Ok(())
    }
}

trait AsReqwestHeaders {
    fn as_reqwest_headers(&self) -> Result<HeaderMap>;
}

impl BlobUploader {
    pub fn new(rx: mpsc::UnboundedReceiver<BlobUploaderMessage>, lookup: Arc<RuntimeAST>) -> Self {
        let batch_size = lookup
            .ast
            .env_var("BAML_BLOB_BATCH_SIZE")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10); // Default to 10 blobs per batch

        Self {
            rx,
            lookup,
            queued_blobs: Vec::new(),
            batch_size,
        }
    }

    pub async fn run(&mut self) {
        let mut upload_interval = interval(Duration::from_secs(2));

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
                    // log::info!("Blob uploader received tick");
                    if self.lookup.api_key().is_none() {
                        log::info!("Skipping blob upload because BOUNDARY_API_KEY is not set");
                        continue;
                    }
                    // Process any queued blobs on the timer
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

        log::info!("Processing {} queued blobs", self.queued_blobs.len());
        let blobs_to_upload = std::mem::take(&mut self.queued_blobs);
        let result = self.upload_blob_batch(blobs_to_upload).await;
        if let Err(e) = result {
            log::error!("Failed to upload queued blob batch: {e}");
        }
    }

    async fn upload_blob_batch(
        &self,
        blobs: Vec<super::rpc_converters::blob_storage::BlobWithContent>,
    ) -> Result<()> {
        if blobs.is_empty() {
            return Ok(());
        }

        log::debug!("Uploading {} blobs", blobs.len());

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
        log::debug!("upload_response={upload_response:?}");

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
                payload: blob.content.clone(),
                media_type: blob.metadata.media_type.clone(),
            })
            .collect();

        let batch_file = BlobBatchUploadS3File {
            blobs: upload_items,
        };

        // Upload to S3
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

        let blob_hashes: Vec<String> = blobs_to_upload
            .iter()
            .map(|b| b.metadata.blob_hash.clone())
            .collect();

        match upload_result {
            Ok(_) => {
                log::debug!("Successfully uploaded {} blobs", blobs_to_upload.len());
            }
            Err(e) => {
                log::error!("Failed to upload blob batch to S3: {e}");
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
pub fn publish_trace_event(event: Arc<TraceEventWithMeta>) -> anyhow::Result<()> {
    let Some(channel) = get_publish_channel(false) else {
        return Ok(());
    };
    channel
        .send(PublisherMessage::Trace(event))
        .map_err(|e| e.into())
}

// Note, the library we are using doesnt seem to work well for flushing in Node
// but that's ok since noone uses our wasm build in node for logging.
// https://github.com/whizsid/wasmtimer-rs/issues/26
pub async fn flush() -> anyhow::Result<()> {
    let Some(channel) = get_publish_channel(false) else {
        return Ok(());
    };
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    if let Err(e) = channel.send(PublisherMessage::Flush(ack_tx)) {
        return Err(e.into());
    }

    // Set a timeout to avoid waiting indefinitely.
    let timeout_duration = Duration::from_secs(8);

    match timeout(timeout_duration, ack_rx).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => Err(anyhow::anyhow!(
            "Flush timed out after {:?}",
            timeout_duration
        )),
    }
}
