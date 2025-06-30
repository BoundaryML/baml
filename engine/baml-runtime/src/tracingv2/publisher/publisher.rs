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
    ApiEndpoint, BamlSrcUploadS3File, CheckBamlSrcUpload, CheckBamlSrcUploadRequest,
    CreateTraceEventUploadUrl, CreateTraceEventUploadUrlRequest, CreateTraceEventUploadUrlResponse,
    NamedType, S3UploadMetadata, TraceEventBatch, TypeDefinition, TypeDefinitionSource,
    TypeReference,
};
use baml_types::{
    tracing::events::{TraceData, TraceEvent},
    BamlValueWithMeta, FieldType, HasFieldType,
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

use super::rpc_converters::{to_rpc_event, IntoRpcEvent, TypeLookup};
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

/// Global publisher channel.
/// When the module is first used, we create an unbounded channel and then spawn the publisher task.
static PUBLISHING_CHANNEL: OnceCell<mpsc::UnboundedSender<PublisherMessage>> = OnceCell::new();
#[cfg(not(target_arch = "wasm32"))]
static PUBLISHING_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();

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
                println!(
                    "error: {:#?}, url: {}, path: {}",
                    e,
                    self.base_url(),
                    TEndpoint::path()
                );
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

pub fn start_publisher(
    lookup: Arc<AstSignatureWrapper>,
    #[cfg(not(target_arch = "wasm32"))] rt: Arc<tokio::runtime::Runtime>,
) {
    if lookup.env_var("BOUNDARY_API_KEY").is_none() {
        log::debug!("Skipping publisher because BOUNDARY_API_KEY is not set");
        return;
    }
    log::debug!("Starting publisher");

    let lookup = Arc::new(RuntimeAST {
        ast: lookup,
        client: reqwest::Client::new(),
    });

    // Use get_or_init to ensure thread-safe initialization
    let channel = PUBLISHING_CHANNEL.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded_channel::<PublisherMessage>();
        let mut publisher = TracePublisher::new(rx, lookup.clone());

        #[cfg(not(target_arch = "wasm32"))]
        {
            let handle = rt.spawn(async move { publisher.run().await });
            PUBLISHING_TASK.get_or_init(|| Arc::new(handle));
        }

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            publisher.run().await;
        });

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
}

impl TracePublisher {
    pub fn new(rx: mpsc::UnboundedReceiver<PublisherMessage>, lookup: Arc<RuntimeAST>) -> Self {
        let batch_size = lookup
            .ast
            .env_var("BAML_TRACE_BATCH_SIZE")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(12);

        Self {
            rx,
            batch_size,
            lookup,
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
                            }

                        },
                        PublisherMessage::Flush(flush_ack) => {
                            // Flush the current buffer if it has any pending events.
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
                            // Signal flush completion.
                            let _ = flush_ack.send(());
                        },
                        PublisherMessage::Shutdown(shutdown_ack) => {
                            if !buffer.is_empty() {
                                self.process_batch(std::mem::take(&mut buffer)).await;
                            }
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

        tracing::info!(
            "Checking if BAML source upload is needed (hash: {})",
            baml_src_hash
        );

        // Check if we should upload
        let check_response = match lookup
            .api_request::<CheckBamlSrcUpload>(CheckBamlSrcUploadRequest { baml_src_hash })
            .await
        {
            Ok(response) => response,
            Err(e) => {
                tracing::error!("Failed to check BAML source upload status: {}", e);
                return Err(e.into());
            }
        };
        tracing::info!("check_response={:?}", check_response);

        if !check_response.should_upload {
            tracing::info!("BAML source already uploaded, skipping");
            return Ok(());
        }

        tracing::info!("Uploading BAML source");

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

        tracing::info!("Successfully uploaded BAML source");
        Ok(())
    }

    async fn process_batch(&self, batch: Vec<Arc<TraceEventWithMeta>>) {
        let batch_result = self.process_batch_with_splitting(batch).await;
        if let Err(e) = batch_result {
            baml_log::debug!("Failed to upload trace events after retries: {:?}", e);
        }
    }

    /// Process a batch with automatic splitting on failure.
    /// If a batch fails to upload, we'll recursively split it in half and retry.
    /// This helps with payload size limits, rate limiting, and transient network issues.
    async fn process_batch_with_splitting(
        &self,
        batch: Vec<Arc<TraceEventWithMeta>>,
    ) -> Result<()> {
        // Get minimum batch size from env var, default to 1 (individual events)
        let min_batch_size = self
            .lookup
            .ast
            .env_var("BAML_MIN_BATCH_SIZE")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1);

        self.process_batch_recursive(batch, min_batch_size).await
    }

    /// Recursively process batches, splitting on failure until we reach minimum size.
    async fn process_batch_recursive(
        &self,
        batch: Vec<Arc<TraceEventWithMeta>>,
        min_batch_size: usize,
    ) -> Result<()> {
        // Try to upload the batch
        match self.process_batch_impl(batch.clone()).await {
            Ok(()) => {
                tracing::debug!("Successfully uploaded batch of {} events", batch.len());
                Ok(())
            }
            Err(e) => {
                log::info!("Failed to upload batch of {} events: {}", batch.len(), e);
                // If batch size is at or below minimum, give up
                if batch.len() <= min_batch_size {
                    log::info!(
                        "Failed to upload single/minimum batch of {} events: {}",
                        batch.len(),
                        e
                    );
                    return Err(e);
                }

                // Split the batch in half and retry each half
                let mid = batch.len() / 2;
                let (first_half, second_half) = batch.split_at(mid);

                tracing::debug!(
                    "Batch upload failed (size: {}), splitting into {} and {} events: {}",
                    batch.len(),
                    first_half.len(),
                    second_half.len(),
                    e
                );

                // Process both halves recursively with Box::pin
                let first_result =
                    Box::pin(self.process_batch_recursive(first_half.to_vec(), min_batch_size))
                        .await;
                let second_result =
                    Box::pin(self.process_batch_recursive(second_half.to_vec(), min_batch_size))
                        .await;

                // If either half failed, propagate the error
                match (first_result, second_result) {
                    (Ok(()), Ok(())) => {
                        tracing::debug!("Successfully uploaded split batches");
                        Ok(())
                    }
                    (Err(e1), Ok(())) => {
                        log::info!("First half failed: {e1}");
                        Err(e1)
                    }
                    (Ok(()), Err(e2)) => {
                        log::info!("Second half failed: {e2}");
                        Err(e2)
                    }
                    (Err(e1), Err(e2)) => {
                        log::debug!("Both halves failed - first: {e1}, second: {e2}");
                        Err(e1) // Return the first error
                    }
                }
            }
        }
    }

    /// Process a batch of events.
    ///
    /// In this example we:
    ///   1. Serialize the events into JSON.
    ///   2. Append the JSON to a file (using async file I/O on macOS).
    ///   3. Post the JSON to an HTTP API with up to 3 retries.
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
            .api_request::<CreateTraceEventUploadUrl>(CreateTraceEventUploadUrlRequest {})
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

        Ok(())
    }
}

trait AsReqwestHeaders {
    fn as_reqwest_headers(&self) -> Result<HeaderMap>;
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
