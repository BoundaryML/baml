use std::{
    cell::RefCell,
    sync::mpsc::{Receiver, Sender, TryRecvError},
};

// use crate::log_callback_event::LogEvent
use anyhow::Result;
use web_time::Duration;

use crate::{
    on_log_event::{LogEvent, LogEventCallbackSync, LogEventMetadata},
    tracing::api_wrapper::core_types::{MetadataType, Template},
};

use super::api_wrapper::{core_types::LogSchema, APIWrapper, BoundaryAPI};

enum TxEventSignal {
    Stop,
    Flush,
    Submit(LogSchema),
}

enum RxEventSignal {
    Done,
}

async fn process_batch_async(api_config: &APIWrapper, batch: Vec<LogSchema>) {
    log::info!("Processing batch of size: {}", batch.len());
    for work in batch {
        match api_config.log_schema(&work).await {
            Ok(_) => {
                log::debug!(
                    "Successfully sent log schema: {} - {:?}",
                    work.event_id,
                    work.context.event_chain.last()
                );
            }
            Err(e) => {
                log::warn!("Unable to emit BAML logs: {}", e);
            }
        }
    }
}

fn process_batch(rt: &tokio::runtime::Runtime, api_config: &APIWrapper, batch: Vec<LogSchema>) {
    rt.block_on(process_batch_async(api_config, batch));
}

fn batch_processor(
    api_config: APIWrapper,
    rx: Receiver<TxEventSignal>,
    tx: Sender<RxEventSignal>,
    max_batch_size: usize,
) {
    let api_config = &api_config;
    let mut batch = Vec::with_capacity(max_batch_size);
    let mut now = std::time::Instant::now();
    let rt = tokio::runtime::Runtime::new().unwrap();
    loop {
        // Try to fill the batch up to max_batch_size
        let (batch_full, flush, exit) = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(TxEventSignal::Submit(work)) => {
                batch.push(work);
                (batch.len() >= max_batch_size, false, false)
            }
            Ok(TxEventSignal::Flush) => (false, true, false),
            Ok(TxEventSignal::Stop) => (false, false, true),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => (false, false, false),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => (false, false, true),
        };

        let time_trigger = now.elapsed().as_millis() >= 1000;

        let should_process_batch =
            (batch_full || flush || exit || time_trigger) && !batch.is_empty();

        // Send events every 1 second or when the batch is full
        if should_process_batch {
            process_batch(&rt, api_config, std::mem::take(&mut batch));
        }

        if should_process_batch || time_trigger {
            now = std::time::Instant::now();
        }

        if flush {
            match tx.send(RxEventSignal::Done) {
                Ok(_) => {}
                Err(e) => {
                    println!("Error sending flush signal: {:?}", e);
                }
            }
        }
        if exit {
            return;
        }
    }
}

pub(super) struct ThreadedTracer {
    tx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Sender<TxEventSignal>>>,
    rx: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<RxEventSignal>>>,
    #[allow(dead_code)]
    join_handle: std::thread::JoinHandle<()>,
    log_event_callback: std::sync::Arc<std::sync::Mutex<Option<LogEventCallbackSync>>>,
}

impl ThreadedTracer {
    fn start_worker(
        api_config: APIWrapper,
        max_batch_size: usize,
    ) -> (
        std::sync::mpsc::Sender<TxEventSignal>,
        std::sync::mpsc::Receiver<RxEventSignal>,
        std::thread::JoinHandle<()>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel();
        let join_handle =
            std::thread::spawn(move || batch_processor(api_config, rx, stop_tx, max_batch_size));

        (tx, stop_rx, join_handle)
    }

    pub fn new(
        api_config: &APIWrapper,
        max_batch_size: usize,
        log_event_callback: Option<Box<dyn Fn(LogSchema) -> Result<()> + Send + Sync>>,
    ) -> Self {
        let (tx, rx, join_handle) = Self::start_worker(api_config.clone(), max_batch_size);
        Self {
            tx: std::sync::Arc::new(std::sync::Mutex::new(tx)),
            rx: std::sync::Arc::new(std::sync::Mutex::new(rx)),
            join_handle,
            log_event_callback: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn flush(&self) -> Result<()> {
        self.tx
            .lock()
            .map_err(|e| anyhow::anyhow!("Error flushing BatchProcessor: {:?}", e))?
            .send(TxEventSignal::Flush)?;

        loop {
            match self.rx.lock() {
                Ok(rx) => match rx.try_recv() {
                    Ok(RxEventSignal::Done) => return Ok(()),
                    Err(TryRecvError::Empty) => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(TryRecvError::Disconnected) => {
                        return Err(anyhow::anyhow!("BatchProcessor worker thread disconnected"))
                    }
                },
                Err(e) => return Err(anyhow::anyhow!("Error flushing BatchProcessor: {:?}", e)),
            }
        }
    }

    pub fn set_log_event_callback(&self, log_event_callback: LogEventCallbackSync) {
        // Get a mutable lock on the log_event_callback
        let mut callback_lock = self.log_event_callback.lock().unwrap();

        // Set the new callback
        *callback_lock = Some(log_event_callback);
    }

    pub fn submit(&self, event: LogSchema) -> Result<()> {
        log::debug!("Submitting work {:#?}", event.event_id);

        let callback = self.log_event_callback.lock().unwrap();
        if let Some(ref callback) = *callback {
            let event = event.clone();
            let llm_output_model = event.metadata.as_ref().and_then(|m| match m {
                MetadataType::Single(llm_event) => Some(llm_event),
                // take the last element in the vector
                MetadataType::Multi(llm_events) => llm_events.last().clone(),
            });

            let log_event_result = callback(LogEvent {
                metadata: LogEventMetadata {
                    event_id: event.event_id.clone(),
                    parent_id: event.parent_event_id.clone(),
                    root_event_id: event.root_event_id.clone(),
                },
                prompt: llm_output_model.and_then(|llm_event| {
                    match llm_event.clone().input.prompt.template {
                        Template::Single(text) => Some(text),
                        Template::Multiple(chat_prompt) => {
                            serde_json::to_string_pretty(&chat_prompt).ok().or_else(|| {
                                log::info!(
                                    "Failed to serialize chat prompt for event {}",
                                    event.event_id
                                );
                                None
                            })
                        }
                    }
                }),
                raw_output: llm_output_model.and_then(|llm_event| {
                    llm_event
                        .clone()
                        .output
                        .and_then(|output| Some(output.raw_text))
                }),
                parsed_output: event.io.output.and_then(|output| {
                    serde_json::to_string(&output.value).ok().or_else(|| {
                        log::info!(
                            "Failed to serialize output value for event {}",
                            event.event_id
                        );
                        None
                    })
                }),
                start_time: event.context.start_time,
            });

            if log_event_result.is_err() {
                log::error!(
                    "Error calling log_event_callback for event id: {}",
                    event.event_id
                );
            }

            log_event_result?;
        }

        let tx = self
            .tx
            .lock()
            .map_err(|e| anyhow::anyhow!("Error submitting work: {:?}", e))?;
        tx.send(TxEventSignal::Submit(event))?;
        Ok(())
    }
}
