use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use anyhow::Result;
use web_time::Duration;

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

    pub fn new(api_config: &APIWrapper, max_batch_size: usize) -> Self {
        let (tx, rx, join_handle) = Self::start_worker(api_config.clone(), max_batch_size);
        Self {
            tx: std::sync::Arc::new(std::sync::Mutex::new(tx)),
            rx: std::sync::Arc::new(std::sync::Mutex::new(rx)),
            join_handle,
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

    pub fn submit(&self, event: LogSchema) -> Result<()> {
        log::info!("Submitting work {}", event.event_id);
        let tx = self
            .tx
            .lock()
            .map_err(|e| anyhow::anyhow!("Error submitting work: {:?}", e))?;
        tx.send(TxEventSignal::Submit(event))?;
        Ok(())
    }
}
