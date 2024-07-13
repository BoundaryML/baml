use anyhow::Result;
use futures::future::join_all;
use std::{
    cell::RefCell,
    ops::DerefMut,
    sync::{
        mpsc::{self, RecvTimeoutError},
        Arc, Mutex,
    },
};
use tokio::{
    runtime::{self, Handle},
    sync::{oneshot, watch},
};
use web_time::{Duration, Instant};

use crate::{
    on_log_event::{LogEvent, LogEventCallbackSync, LogEventMetadata},
    tracing::api_wrapper::core_types::{ContentPart, MetadataType, Template, ValueType},
    TraceStats,
};

use super::api_wrapper::{core_types::LogSchema, APIConfig, APIWrapper, BoundaryAPI};

const MAX_TRACE_SEND_CONCURRENCY: usize = 10;

enum TxEventSignal {
    Noop,
    Stop(oneshot::Sender<TraceStats>),
    Submit(LogSchema),
}

enum ProcessorStatus {
    Active,
    Done(u128),
}

struct DeliveryThread {
    api_config: Arc<APIWrapper>,
    max_batch_size: usize,
    max_concurrency: Arc<tokio::sync::Semaphore>,
    stats: TraceStats,
}

impl DeliveryThread {
    // TODO: this needs to submit stuff to the runtime
    fn process_batch(&self, rt: &Handle, batch: Vec<LogSchema>) {
        let work = batch
            .into_iter()
            .map(|work| {
                let api_config = self.api_config.clone();
                let semaphore = self.max_concurrency.clone();
                let stats = self.stats.clone();
                stats.guard().send();

                let stats_clone = stats.clone();
                async move {
                    let guard = stats_clone.guard();
                    let Ok(_acquired) = semaphore.acquire().await else {
                        log::warn!(
                            "Failed to acquire semaphore because it was closed - not sending span"
                        );
                        return;
                    };
                    match api_config.log_schema(&work).await {
                        Ok(_) => {
                            guard.done();
                            log::debug!(
                                "Successfully sent log schema: {} - {:?}",
                                work.event_id,
                                work.context.event_chain.last()
                            );
                        }
                        Err(e) => {
                            log::warn!("Unable to emit BAML logs: {:#?}", e);
                        }
                    }
                }
            })
            .collect::<Vec<_>>();

        rt.spawn(join_all(work));
    }

    fn run(&mut self, mut span_rx: std::sync::mpsc::Receiver<TxEventSignal>) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name("tracing-delivery")
            .build()
            .expect("Failed to start tracing thread");

        log::debug!("[DeliveryThread] starting");
        for i in 0..10 {
            log::trace!(
                "[DeliveryThread] startup loop: {} is receiver closed? {}",
                i,
                "no idea"
            );
            std::thread::sleep(Duration::from_secs(1));
        }
        loop {
            log::trace!("[DeliveryThread] looping");
            // Try to fill the batch up to max_batch_size
            // let exit = match span_rx.blocking_recv() {
            //     Some(TxEventSignal::Submit(work)) => {
            //         log::debug!("[DeliveryThread] SUBMIT received");
            //         self.stats.guard().submit();
            //         self.process_batch(rt.handle(), vec![work]);

            //         false
            //     }
            //     Some(TxEventSignal::Noop) => {
            //         log::trace!("[DeliveryThread] NOOP recv");
            //         false
            //     }
            //     Some(TxEventSignal::Stop(sender)) => {
            //         let _ = sender.send(self.stats.clone());
            //         log::trace!("[DeliveryThread] STOP recv");
            //         true
            //     }
            //     None => true,
            // };
            let exit = match span_rx.recv_timeout(Duration::from_secs(1)) {
                Ok(TxEventSignal::Submit(work)) => {
                    log::debug!("[DeliveryThread] SUBMIT received");
                    self.process_batch(rt.handle(), vec![work]);

                    false
                }
                Ok(TxEventSignal::Noop) => {
                    log::trace!("[DeliveryThread] NOOP recv");
                    false
                }
                Ok(TxEventSignal::Stop(sender)) => {
                    log::trace!("[DeliveryThread] STOP recv");
                    let _ = sender.send(self.stats.clone());
                    true
                }
                Err(RecvTimeoutError::Timeout) => {
                    log::trace!("[DeliveryThread] Error receiving from channel: timeout");
                    false
                }
                Err(RecvTimeoutError::Disconnected) => {
                    log::trace!("[DeliveryThread] Error receiving from channel: disconnected");
                    true
                }
            };

            if exit {
                log::trace!("[DeliveryThread] exiting");
                return;
            }
        }
    }
}

pub(super) struct ThreadedTracer {
    api_config: Arc<APIWrapper>,
    // span_tx: tokio::sync::mpsc::UnboundedSender<TxEventSignal>,
    span_tx: std::sync::mpsc::Sender<TxEventSignal>,
    // stop_rx: watch::Receiver<ProcessorStatus>,
    #[allow(dead_code)]
    join_handle: Option<std::thread::JoinHandle<()>>,
    log_event_callback: Arc<Mutex<Option<LogEventCallbackSync>>>,
    stats: TraceStats,
}

impl ThreadedTracer {
    pub fn new(api_config: &APIWrapper, max_batch_size: usize, stats: TraceStats) -> Self {
        // let (span_tx, span_rx) = tokio::sync::mpsc::unbounded_channel();
        let (span_tx, span_rx) = std::sync::mpsc::channel();
        // let (stop_tx, stop_rx) = watch::channel(ProcessorStatus::Active);

        let api_config = Arc::new(api_config.clone());

        let mut t = DeliveryThread {
            api_config: api_config.clone(),
            max_batch_size,
            max_concurrency: tokio::sync::Semaphore::new(MAX_TRACE_SEND_CONCURRENCY).into(),
            stats: stats.clone(),
        };

        std::thread::spawn(move || {
            t.run(span_rx);
        });

        Self {
            api_config,
            span_tx,
            //stop_rx,
            join_handle: None,
            log_event_callback: Arc::new(Mutex::new(None)),
            stats,
        }
    }

    pub fn flush(&self) -> Result<()> {
        // let id = std::time::SystemTime::now()
        //     .duration_since(std::time::UNIX_EPOCH)
        //     .unwrap()
        //     .as_millis();
        // log::debug!("Asking delivery thread to flush events");
        // self.span_tx.send(TxEventSignal::Flush(id))?;

        // let flush_start = Instant::now();

        // while flush_start.elapsed() < Duration::from_secs(60) {
        //     {
        //         match *self.stop_rx.borrow() {
        //             ProcessorStatus::Active => {}
        //             ProcessorStatus::Done(r_id) if r_id >= id => {
        //                 return Ok(());
        //             }
        //             ProcessorStatus::Done(id) => {
        //                 // Old flush, ignore
        //             }
        //         }
        //     }
        //     std::thread::sleep(Duration::from_millis(100));
        // }

        // anyhow::bail!("BatchProcessor worker thread did not finish in time")
        self.shutdown()
    }

    // pub fn shutdown(&self) -> Result<()> {
    //     let mut locked = self.runtime.lock().unwrap();
    //     match *locked {
    //         Some(ref t) => log::debug!(
    //             "Asking delivery thread to stop, runtime status is {:#?}",
    //             t.metrics()
    //         ),
    //         None => {
    //             log::debug!("Asking delivery thread to stop, runtime has already been shutdown")
    //         }
    //     }
    //     self.span_tx.send(TxEventSignal::Stop)?;

    //     let Some(runtime) = std::mem::take(locked.deref_mut()) else {
    //         anyhow::bail!("ThreadedTracer has already been shutdown");
    //     };
    //     runtime.shutdown_timeout(Duration::from_secs(13));
    //     Ok(())
    // }

    pub fn shutdown(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();

        self.span_tx.send(TxEventSignal::Stop(tx))?;

        match rx.blocking_recv() {
            Ok(stats) => {
                log::debug!("Received stats from delivery thread");
            }
            Err(e) => {
                log::error!("Error receiving handle from delivery thread: {:?}", e);
            }
        };

        Ok(())
    }

    pub fn set_log_event_callback(&self, log_event_callback: LogEventCallbackSync) {
        // Get a mutable lock on the log_event_callback
        let mut callback_lock = self.log_event_callback.lock().unwrap();

        *callback_lock = Some(log_event_callback);
    }

    pub fn submit(&self, mut event: LogSchema) -> Result<()> {
        log::debug!("submitting NOOPs during trace.submit");
        for _ in 0..3 {
            match self.span_tx.send(TxEventSignal::Noop) {
                Ok(_) => {
                    log::debug!("NOOP sent to delivery thread");
                    if let Some(join_handle) = &self.join_handle {
                        join_handle.thread().unpark();
                    }
                }
                Err(e) => {
                    log::error!("Error sending NOOP to delivery thread: {:?}", e);
                }
            }
        }

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
                                log::debug!(
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
                parsed_output: event.io.output.and_then(|output| match output.value {
                    // so the string value looks something like:
                    // '"[\"d\", \"e\", \"f\"]"'
                    // so we need to unescape it once and turn it into a normal json
                    // and then stringify it to get:
                    // '["d", "e", "f"]'
                    ValueType::String(value) => serde_json::from_str::<serde_json::Value>(&value)
                        .ok()
                        .and_then(|json_value| json_value.as_str().map(|s| s.to_string()))
                        .or_else(|| Some(value)),
                    _ => serde_json::to_string_pretty(&output.value)
                        .ok()
                        .or_else(|| {
                            log::debug!(
                                "Failed to serialize output value for event {}",
                                event.event_id
                            );
                            None
                        }),
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

        // TODO: do the redaction

        // Redact the event
        event = redact_event(event, &self.api_config.config);

        self.span_tx.send(TxEventSignal::Submit(event))?;
        Ok(())
    }
}

fn redact_event(mut event: LogSchema, api_config: &APIConfig) -> LogSchema {
    let redaction_enabled = api_config.log_redaction_enabled();
    let placeholder = api_config.log_redaction_placeholder();

    if !redaction_enabled {
        return event;
    }

    let placeholder = placeholder
        .replace("{root_event.id}", &event.root_event_id)
        .replace("{event.id}", &event.event_id);

    // Redact LLMOutputModel raw_text
    if let Some(metadata) = &mut event.metadata {
        match metadata {
            MetadataType::Single(llm_event) => {
                if let Some(output) = &mut llm_event.output {
                    output.raw_text = placeholder.clone();
                }
            }
            MetadataType::Multi(llm_events) => {
                for llm_event in llm_events {
                    if let Some(output) = &mut llm_event.output {
                        output.raw_text = placeholder.clone();
                    }
                }
            }
        }
    }

    // Redact input IO
    if let Some(input) = &mut event.io.input {
        match &mut input.value {
            ValueType::String(s) => *s = placeholder.clone(),
            ValueType::List(v) => v.iter_mut().for_each(|s| *s = placeholder.clone()),
        }
    }

    // Redact output IO
    if let Some(output) = &mut event.io.output {
        match &mut output.value {
            ValueType::String(s) => *s = placeholder.clone(),
            ValueType::List(v) => v.iter_mut().for_each(|s| *s = placeholder.clone()),
        }
    }

    // Redact LLMEventInput Template
    if let Some(metadata) = &mut event.metadata {
        match metadata {
            MetadataType::Single(llm_event) => {
                redact_template(&mut llm_event.input.prompt.template, &placeholder);
            }
            MetadataType::Multi(llm_events) => {
                for llm_event in llm_events {
                    redact_template(&mut llm_event.input.prompt.template, &placeholder);
                }
            }
        }
    }

    event
}

fn redact_template(template: &mut Template, placeholder: &str) {
    match template {
        Template::Single(s) => *s = placeholder.to_string(),
        Template::Multiple(chats) => {
            for chat in chats {
                for part in &mut chat.content {
                    if let ContentPart::Text(s) = part {
                        *s = placeholder.to_string();
                    }
                }
            }
        }
    }
}
