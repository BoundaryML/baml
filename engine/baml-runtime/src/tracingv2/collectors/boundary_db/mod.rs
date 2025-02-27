use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, timeout};

use baml_types::tracing::events::{FunctionId, TraceData, TraceEvent}; // Assuming TraceEvent is defined
use crate::{
    tracingv2::storage::storage::{FunctionTrackerTrait, BAML_TRACER},
    BamlRuntime,
};

/// Messages sent to the collector task.
pub enum CollectorMsg {
    /// Instruct the collector to gracefully shutdown.
    Shutdown,
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
    // Handle for the main collector task.
    join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    // Channel to send events to the S3 pusher task.
    s3_tx: mpsc::Sender<NetworkMsg>,
    // Handle for the S3 pusher task.
    s3_join_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl FunctionTrackerTrait for Collector {
    fn track_function(&self, fid: FunctionId) {
        log::trace!("Tracking function: {:?}", fid);
        // Increment the global ref count.
        BAML_TRACER.lock().unwrap().inc_ref(&fid);
        // Add to our set.
        self.tracked_ids.insert(fid, 0);
    }

    fn untrack_function(&self, fid: &FunctionId) {
        self.tracked_ids.remove(fid);
    }
}

impl Collector {
    /// Creates a new collector and spawns its background tasks.
    /// `tps` sets the number of update ticks per second.
    pub async fn new(tps: u32, runtime: &BamlRuntime) -> Arc<Self> {
        let hash = runtime.create_hash();

        // Channel for shutdown signaling to the collector task.
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        // Channel for sending event batches to the S3 pusher.
        let (s3_tx, s3_rx) = mpsc::channel(100);

        let collector = Arc::new(Self {
            tracked_ids: Arc::new(DashMap::new()),
            shutdown_tx,
            join_handle: Mutex::new(None),
            s3_tx,
            s3_join_handle: Mutex::new(None),
        });

        // Spawn the main collector task.
        let main_handle = Self::start(Arc::clone(&collector), tps, shutdown_rx);
        {
            let mut join_lock = futures::executor::block_on(collector.join_handle.lock());
            *join_lock = Some(main_handle);
        }

        // Spawn the S3 pusher task.
        let s3_handle = Self::start_s3_pusher(s3_rx);
        {
            let mut s3_join_lock = futures::executor::block_on(collector.s3_join_handle.lock());
            *s3_join_lock = Some(s3_handle);
        }

        collector
    }

    /// Spawns the main collector async task which ticks at the given TPS.
    /// It checks for a shutdown signal on every tick.
    fn start(
        collector: Arc<Self>,
        tps: u32,
        mut shutdown_rx: mpsc::Receiver<CollectorMsg>,
    ) -> tokio::task::JoinHandle<()> {
        let interval = Duration::from_secs(1) / tps;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Listen for a shutdown signal.
                    _ = shutdown_rx.recv() => {
                        collector.update_events().await;
                        break;
                    },
                    // Regular tick: process events.
                    _ = sleep(interval) => {
                        collector.update_events().await;
                    }
                }
            }
        })
    }

    /// Spawns the S3 pusher task that listens for batches of events to push.
    fn start_s3_pusher(
        mut s3_rx: mpsc::Receiver<NetworkMsg>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(msg) = s3_rx.recv().await {
                match msg {
                    NetworkMsg::SendEvents(events) => {
                        // Call the async function to push events to S3.
                        if let Err(e) = push_events_to_s3(events).await {
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

    /// Initiates a graceful shutdown of both the collector and S3 pusher tasks.
    /// Sends a shutdown signal and awaits task completion with a timeout.
    pub async fn shutdown(&self, timeout_duration: Duration) {
        // Send the shutdown signal to the main collector task.
        if let Err(e) = self.shutdown_tx.send(CollectorMsg::Shutdown).await {
            log::error!("Failed to send shutdown signal: {:?}", e);
        }
        // Wait for the main collector task to finish.
        if let Some(handle) = { let mut guard = self.join_handle.lock().await; guard.take() } {
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
        if let Some(s3_handle) = { let mut guard = self.s3_join_handle.lock().await; guard.take() } {
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

/// A placeholder async function simulating pushing events to S3.
/// Replace this with your actual S3 upload logic.
async fn push_events_to_s3(events: Vec<Arc<TraceEvent>>) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Pushing {} events to S3", events.len());
    // Simulate network delay.
    sleep(Duration::from_millis(100)).await;
    // TODO: implement real S3 push logic here.
    Ok(())
}
