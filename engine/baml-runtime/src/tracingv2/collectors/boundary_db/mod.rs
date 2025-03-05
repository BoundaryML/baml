mod boundary_api;

use anyhow::Result;
use baml_types::rpc::upload_baml_src::{BamlTypeDefinition, BamlTypeId, BamlTypeReference};
use baml_types::rpc::{self, StudioTraceEventBatch};
use boundary_api::ApiClient;
use dashmap::DashMap;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, timeout};

use crate::runtime_interface::BoundaryCloudInterface;
use crate::TOKIO_SINGLETON;
use crate::{
    tracingv2::storage::storage::{FunctionTrackerTrait, BAML_TRACER},
    BamlRuntime,
};
use baml_types::tracing::events::{FunctionId, TraceData, TraceEvent}; // Assuming TraceEvent is defined

/// Messages sent to the collector task.
pub enum CollectorMsg {
    /// Instruct the collector to gracefully shutdown.
    Shutdown,
    Flush,
}

pub enum CollectorMsgReply {
    /// Reply to a shutdown signal.
    Done,
}

enum NetworkMsg {
    /// Send a batch of events to the S3 pusher task.
    SendEvents(Vec<Arc<TraceEvent>>),
    /// Send a shutdown signal to the S3 pusher task.
    Shutdown,
}

#[derive(Debug)]
pub struct Collector {
    tracked_ids: Arc<DashMap<FunctionId, usize>>,
    shutdown_tx: mpsc::Sender<CollectorMsg>,
    shutdown_rx2: Arc<Mutex<mpsc::Receiver<CollectorMsgReply>>>,

    // Channel to send events to the S3 pusher task.
    s3_tx: mpsc::Sender<NetworkMsg>,
    // Handle for the S3 pusher task.
    config: Arc<tokio::sync::Mutex<BoundaryStudioConfig>>,

    // Handle for the main collector task.
    s3_join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl FunctionTrackerTrait for Collector {
    fn track_function(&self, fid: FunctionId) {
        log::info!("Tracking function: {:?}", fid);
        // Increment the global ref count.
        BAML_TRACER.lock().unwrap().inc_ref(&fid);
        // Add to our set.
        self.tracked_ids.insert(fid, 0);
    }

    fn untrack_function(&self, fid: &FunctionId) {
        log::info!("Untracking function: {:?}", fid);
        self.tracked_ids.remove(fid);
    }

    fn name(&self) -> String {
        // let config = self.config.lock().unwrap();
        // format!("BoundaryStudioCollector({})", config.project_name)
        "BoundaryStudioCollector(project name unknown, needs async lock)".to_string()
    }
}

pub struct BoundaryStudioConfigBuilder {
    /// The base URL for the Boundary Studio API.
    pub base_url: Option<String>,
    /// The project id for the Boundary Studio project.
    pub project_id: String,
    /// The API key for the Boundary Studio project.
    pub api_key: String,
    /// How often to push events to the backend.
    /// Slower == bigger batches
    /// Faster == smaller batches (more network calls)
    pub update_interval: Duration,
}

#[derive(Debug, Clone)]
struct BoundaryStudioConfig {
    project_name: String,
    api_client: ApiClient,
    baml_src_lookups: rpc::UploadBamlSrcRequest,
}

impl BoundaryStudioConfigBuilder {
    async fn build(self, runtime: &BamlRuntime) -> Result<BoundaryStudioConfig> {
        let api_client = ApiClient::new(
            self.base_url
                .as_deref()
                .unwrap_or("https://api.boundaryml.com"),
            &self.project_id,
            Some(self.api_key),
        );

        let baml_src_blob = runtime
            .boundary_cloud_interface()
            .to_boundary_upload_request(self.project_id.clone());

        let project_info = api_client
            .post(
                boundary_api::GetBamlSrcUploadStatus,
                &baml_src_blob.to_get_baml_src_upload_status_request(),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get project info: {:?}", e))?;

        if matches!(
            project_info.status,
            rpc::upload_baml_src::BamlSrcUploadStatus::DoesNotExist
        ) {
            api_client
                .post(boundary_api::UploadBamlSrc, &baml_src_blob)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to upload baml src: {:?}", e))?;
        }

        Ok(BoundaryStudioConfig {
            project_name: "project123".to_string(),
            api_client,
            baml_src_lookups: baml_src_blob,
        })
    }
}
impl Collector {
    /// Creates a new collector and spawns its background tasks.
    /// `tps` sets the number of update ticks per second.
    pub async fn new(
        runtime: &BamlRuntime,
        config: BoundaryStudioConfigBuilder,
    ) -> Result<Arc<Self>> {
        let update_interval = config.update_interval;
        let config = config.build(runtime).await?;

        // Channel for shutdown signaling to the collector task.
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let (shutdown_tx2, shutdown_rx2) = mpsc::channel(1);
        // Channel for sending event batches to the S3 pusher.
        let (s3_tx, s3_rx) = mpsc::channel(100);

        let collector = Arc::new(Self {
            tracked_ids: Arc::new(DashMap::new()),
            shutdown_tx,
            shutdown_rx2: Arc::new(Mutex::new(shutdown_rx2)),
            join_handle: Mutex::new(None),
            s3_tx,
            s3_join_handle: Mutex::new(None),
            config: Arc::new(tokio::sync::Mutex::new(config)),
        });

        // Spawn the main collector task.
        let main_handle = Self::start(
            Arc::clone(&collector),
            update_interval,
            shutdown_rx,
            shutdown_tx2,
        );
        {
            let mut join_lock = futures::executor::block_on(collector.join_handle.lock());
            *join_lock = Some(main_handle);
        }

        // Spawn the S3 pusher task.
        let s3_handle = Self::start_s3_pusher(s3_rx, collector.config.clone());
        {
            let mut s3_join_lock = futures::executor::block_on(collector.s3_join_handle.lock());
            *s3_join_lock = Some(s3_handle);
        }

        Ok(collector)
    }

    /// Spawns the main collector async task which ticks at the given TPS.
    /// It checks for a shutdown signal on every tick.
    fn start(
        collector: Arc<Self>,
        update_interval: Duration,
        mut shutdown_rx: mpsc::Receiver<CollectorMsg>,
        shutdown_tx: mpsc::Sender<CollectorMsgReply>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Listen for a shutdown signal.
                    event = shutdown_rx.recv() => {
                        match event {
                            Some(CollectorMsg::Shutdown) => {
                                collector.update_events().await;
                                break;
                            },
                            Some(CollectorMsg::Flush) => {
                                collector.update_events().await;
                                match shutdown_tx.send(CollectorMsgReply::Done).await {
                                    Ok(_) => {},
                                    Err(e) => {
                                        log::error!("Failed to send finished acknowledgement: {:?}", e);
                                    }
                                }
                            },
                            None => todo!(),
                        }
                    },
                    // Regular tick: process events.
                    _ = sleep(update_interval) => {
                        collector.update_events().await;
                    }
                }
            }
        })
    }

    /// Spawns the S3 pusher task that listens for batches of events to push.
    fn start_s3_pusher(
        mut s3_rx: mpsc::Receiver<NetworkMsg>,
        config: Arc<tokio::sync::Mutex<BoundaryStudioConfig>>,
    ) -> tokio::task::JoinHandle<()> {
        // let local = tokio::task::LocalSet::new();
        tokio::spawn(async move {
            while let Some(msg) = s3_rx.recv().await {
                match msg {
                    NetworkMsg::SendEvents(events) => {
                        // Call the async function to push events to S3.
                        if let Err(e) = push_events_to_s3(events, &config).await {
                            log::error!("Failed to push events to S3: {:?}", e);
                        }
                    }
                    NetworkMsg::Shutdown => {
                        break;
                    }
                }
            }
            log::info!("S3 pusher task shutting down.");
        })
    }

    /// Processes new events from the tracer and cleans up finished function events.
    /// Also sends any gathered events to the S3 pusher task.
    async fn update_events(&self) {
        let events = {
            let tracer = BAML_TRACER.lock().unwrap();
            self.tracked_ids
                .iter_mut()
                .flat_map(|mut kv| {
                    if let Some(events) = tracer.get_events(kv.key()) {
                        // Get events beyond the last processed index.
                        let last_event_index = *kv.value();
                        let new_events = events
                            .iter()
                            .skip(last_event_index)
                            .cloned()
                            .collect::<Vec<_>>();
                        *kv.value_mut() = new_events.len();
                        new_events
                    } else {
                        vec![]
                    }
                })
                .collect::<Vec<_>>()
        };

        // Identify finished function events and untrack them.
        let finished_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.content, TraceData::FunctionEnd(_)))
            .map(|e| &e.span_id)
            .collect();
        for fid in finished_events {
            self.untrack_function(fid);
        }

        // If there are events to push, send them to the S3 pusher task.
        if !events.is_empty() {
            if let Err(e) = self.s3_tx.send(NetworkMsg::SendEvents(events)).await {
                log::error!("Failed to send events to S3 pusher: {:?}", e);
            }
        }
    }

    pub async fn flush(&self, timeout_duration: Duration) {
        if let Err(e) = self.shutdown_tx.send(CollectorMsg::Flush).await {
            log::error!("Failed to send flush signal: {:?}", e);
        };
        let mut shutdown_rx = self.shutdown_rx2.lock().await;
        tokio::select! {
            _ = shutdown_rx.recv() => {
                log::info!("Flush completed.");
            },
            _ = sleep(timeout_duration) => {
                log::warn!("Timeout while waiting for the flush to complete.");
            }
        }
    }

    /// Initiates a graceful shutdown of both the collector and S3 pusher tasks.
    /// Sends a shutdown signal and awaits task completion with a timeout.
    async fn shutdown(&self, timeout_duration: Duration) {
        // Send the shutdown signal to the main collector task.
        if let Err(e) = self.shutdown_tx.send(CollectorMsg::Shutdown).await {
            log::error!("Failed to send shutdown signal: {:?}", e);
        }
        // Wait for the main collector task to finish.
        if let Some(handle) = {
            let mut guard = self.join_handle.lock().await;
            guard.take()
        } {
            match timeout(timeout_duration, handle).await {
                Ok(result) => {
                    if let Err(e) = result {
                        log::error!("Collector task error: {:?}", e);
                    }
                }
                Err(_) => {
                    log::warn!("Timeout while waiting for the collector task to shut down.");
                }
            }
        }

        // Signal the S3 pusher to shut down by closing its channel.
        self.s3_tx.send(NetworkMsg::Shutdown).await.unwrap();
        // Wait for the S3 pusher task to finish.
        if let Some(s3_handle) = {
            let mut guard = self.s3_join_handle.lock().await;
            guard.take()
        } {
            match timeout(timeout_duration, s3_handle).await {
                Ok(result) => {
                    if let Err(e) = result {
                        log::error!("S3 pusher task error: {:?}", e);
                    }
                }
                Err(_) => {
                    log::warn!("Timeout while waiting for the S3 pusher task to shut down.");
                }
            }
        }

        if !self.tracked_ids.is_empty() {
            log::warn!("Some functions are still being tracked and will be dropped/canceled.");
        }
    }
}

struct IdRewriter {
    type_id_map: HashMap<String, BamlTypeId>,
}

impl IdRewriter {
    fn new(baml_src_lookups: &rpc::UploadBamlSrcRequest) -> Self {
        let mut type_id_map: HashMap<String, BamlTypeId> = HashMap::new();
        for td in baml_src_lookups.type_definitions.iter() {
            match td {
                BamlTypeDefinition::Class(cd) => {
                    type_id_map.insert(cd.type_id.0.to_string(), cd.type_id.clone());
                }
                BamlTypeDefinition::Enum(ed) => {
                    type_id_map.insert(ed.type_id.0.to_string(), ed.type_id.clone());
                }
                BamlTypeDefinition::TypeAlias(td) => {
                    type_id_map.insert(td.type_id.0.to_string(), td.type_id.clone());
                }
            }
        }
        Self { type_id_map }
    }

    fn rewrite(&self, t: &mut BamlTypeReference) {
        match t {
            BamlTypeReference::Class { type_id } => {
                if let Some(id) = self.type_id_map.get(type_id) {
                    *t = BamlTypeReference::Class {
                        type_id: id.0.to_string(),
                    };
                }
            }
            BamlTypeReference::Enum { type_id } => {
                if let Some(id) = self.type_id_map.get(type_id) {
                    *t = BamlTypeReference::Enum {
                        type_id: id.0.to_string(),
                    };
                }
            }
            BamlTypeReference::TypeAlias { type_id } => {
                if let Some(id) = self.type_id_map.get(type_id) {
                    *t = BamlTypeReference::TypeAlias {
                        type_id: id.0.to_string(),
                    };
                }
            }
            BamlTypeReference::Array { items } => {
                self.rewrite(items);
            }
            BamlTypeReference::Map { key, value } => {
                self.rewrite(key);
                self.rewrite(value);
            }
            BamlTypeReference::Union { any_of } => {
                for item in any_of {
                    self.rewrite(item);
                }
            }
            BamlTypeReference::Tuple { items } => {
                for item in items {
                    self.rewrite(item);
                }
            }
            BamlTypeReference::Null => {}
            BamlTypeReference::Int => {}
            BamlTypeReference::Bool => {}
            BamlTypeReference::Float => {}
            BamlTypeReference::String => {}
            BamlTypeReference::Media(m) => {}
            BamlTypeReference::Literal(l) => {}
        }
    }
}

/// A placeholder async function simulating pushing events to S3.
/// Replace this with your actual S3 upload logic.
async fn push_events_to_s3(
    events: Vec<Arc<TraceEvent>>,
    config: &Arc<tokio::sync::Mutex<BoundaryStudioConfig>>,
) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Convert from TraceEvent to the JSON format expected by the Boundary Studio API.
    let locked_config = config.lock().await;
    let baml_src_lookups = &locked_config.baml_src_lookups;
    // lookups contains all the unique ids for the functions, classes, enums, and type aliases in the BAML src.
    // can find my name match.
    log::info!("Pushing {} events to S3", events.len());

    let id_rewriter = IdRewriter::new(baml_src_lookups);

    // Simulate network delay.
    let request = rpc::TraceEventUploadRequest {
        trace_event_batch: StudioTraceEventBatch {
            project_id: locked_config.project_name.clone(),
            events: events
                .into_iter()
                .filter_map(|e| match &e.content {
                    TraceData::FunctionStart(start) => Some(baml_types::tracing::rpc::TraceEvent {
                        span_chain: e.span_chain.clone(),
                        tags: e.tags.clone(),
                        timestamp: e.timestamp.clone(),
                        callsite: e.callsite.clone(),
                        verbosity: e.verbosity.clone(),
                        span_id: e.span_id.clone(),
                        event_id: e.event_id.clone(),
                        content: baml_types::tracing::rpc::TraceData::FunctionStart(
                            baml_types::tracing::rpc::FunctionStart {
                                function_id: FunctionId(
                                    baml_src_lookups
                                        .function_definitions
                                        .iter()
                                        .find(|f| {
                                            f.function_id
                                                .0
                                                .name
                                                .starts_with(&start.function_display_name)
                                        })
                                        .expect(
                                            format!(
                                                "function ID not found: {}",
                                                start.function_display_name
                                            )
                                            .as_str(),
                                        )
                                        .function_id
                                        .0
                                        .to_string(),
                                ),
                                function_display_name: start.function_display_name.clone(),
                                args: {
                                    start
                                        .args
                                        .clone()
                                        .into_iter()
                                        .map(|(k, mut v)| {
                                            (
                                                k,
                                                serde_json::to_value(
                                                    v.rewrite_references_to_include_id(&|t| {
                                                        id_rewriter.rewrite(t)
                                                    }),
                                                )
                                                .expect("failed to rewrite type reference"),
                                            )
                                        })
                                        .collect()
                                },
                                options: (),
                            },
                        ),
                    }),
                    TraceData::FunctionEnd(end) => Some(baml_types::tracing::rpc::TraceEvent {
                        span_chain: e.span_chain.clone(),
                        tags: e.tags.clone(),
                        timestamp: e.timestamp.clone(),
                        callsite: e.callsite.clone(),
                        verbosity: e.verbosity.clone(),
                        span_id: e.span_id.clone(),
                        event_id: e.event_id.clone(),
                        content: baml_types::tracing::rpc::TraceData::FunctionEnd(
                            baml_types::tracing::rpc::FunctionEnd {
                                function_id: FunctionId(
                                    baml_src_lookups
                                        .function_definitions
                                        .iter()
                                        .find(|f| {
                                            f.function_id
                                                .0
                                                .name
                                                .starts_with(&end.function_display_name)
                                        })
                                        .expect(
                                            format!(
                                                "function ID not found: {}",
                                                end.function_display_name
                                            )
                                            .as_str(),
                                        )
                                        .function_id
                                        .0
                                        .to_string(),
                                ),
                                function_display_name: end.function_display_name.clone(),
                                result: match &end.result {
                                    Ok(result) => Ok(serde_json::to_value(
                                        result.clone().rewrite_references_to_include_id(&|t| {
                                            id_rewriter.rewrite(t)
                                        }),
                                    )
                                    .expect("failed to rewrite type reference")),
                                    Err(e) => {
                                        Err(anyhow::anyhow!("error occurred inside LLM: {:?}", e))
                                    }
                                },
                            },
                        ),
                    }),
                    other => {
                        tracing::debug!(
                            "Dropping event type: {:?}",
                            std::mem::discriminant(&e.content)
                        );
                        None
                    }
                })
                .collect(),
        },
    };
    locked_config
        .api_client
        .post(boundary_api::UploadTraceEvent, &request)
        .await?;
    // TODO: implement real S3 push logic here.
    Ok(())
}

impl Drop for Collector {
    fn drop(&mut self) {
        log::info!("Dropping boudary studio collector: {}", self.name());
        // Wait up to 5 seconds for the shutdown to complete.
        let fut = self.shutdown(Duration::from_secs(5));
        // Get the current runtime
        match TOKIO_SINGLETON.get().unwrap() {
            Ok(runtime) => {
                runtime.block_on(fut);
            }
            Err(e) => {
                log::error!("Failed to get tokio runtime: {:?}", e);
            }
        }
    }
}
