use anyhow::Result;
use std::{
    sync::mpsc::{Receiver, Sender, TryRecvError},
    time::Duration,
};

use crate::api_wrapper::{api_interface::BoundaryAPI, core_types::LogSchema, APIWrapper};

fn process_batch(api_config: &APIWrapper, batch: Vec<LogSchema>) {
    for work in batch {
        api_config.pretty_print(&work);
        let _ = api_config.log_schema(&work);
    }
}

fn batch_processor(
    api_config: APIWrapper,
    rx: Receiver<TxEventSignal>,
    tx: Sender<RxEventSignal>,
    max_batch_size: usize,
) {
    let api_config = &api_config;
    let mut batch = Vec::with_capacity(max_batch_size);
    loop {
        // Try to fill the batch up to max_batch_size
        match rx.recv_timeout(Duration::from_millis(1000)) {
            Ok(TxEventSignal::Submit(work)) => batch.push(work),
            Ok(TxEventSignal::Flush) => {
                if !batch.is_empty() {
                    process_batch(api_config, std::mem::take(&mut batch));
                }
                tx.send(RxEventSignal::Done);
            }
            Ok(TxEventSignal::Stop) => {
                if !batch.is_empty() {
                    process_batch(api_config, std::mem::take(&mut batch));
                }
                return; // Exit loop and thread
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => { /* No work, continue */ }
            Err(e) => {
                if !batch.is_empty() {
                    process_batch(api_config, std::mem::take(&mut batch));
                }
                return; // Exit loop and thread
            }
        }

        // Process the batch if it's full or after waiting a bit
        if !batch.is_empty() {
            process_batch(api_config, std::mem::take(&mut batch));
        }
    }
}

enum TxEventSignal {
    Stop,
    Flush,
    Submit(LogSchema),
}

enum RxEventSignal {
    Done,
}

pub(super) struct BatchProcessor {
    api_config: APIWrapper,
    tx: Sender<TxEventSignal>,
    rx: Receiver<RxEventSignal>,
    join_handle: Option<std::thread::JoinHandle<()>>,
}

fn start_worker(
    api_config: &APIWrapper,
    max_batch_size: usize,
) -> (
    std::sync::mpsc::Sender<TxEventSignal>,
    std::sync::mpsc::Receiver<RxEventSignal>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let api_config = api_config.clone();
    let join_handle =
        std::thread::spawn(move || batch_processor(api_config, rx, stop_tx, max_batch_size));
    (tx, stop_rx, join_handle)
}

impl BatchProcessor {
    pub fn api(&self) -> &APIWrapper {
        &self.api_config
    }

    pub fn new(api_config: APIWrapper, max_batch_size: usize) -> Self {
        let (tx, rx, join_handle) = start_worker(&api_config, max_batch_size);
        Self {
            api_config,
            tx,
            rx,
            join_handle: Some(join_handle),
        }
    }

    pub fn submit(&self, work: LogSchema) -> Result<()> {
        self.tx.send(TxEventSignal::Submit(work))?;
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        // Send a flush signal to the worker
        self.tx.send(TxEventSignal::Flush)?;

        // Wait for the worker to finish processing the flush
        loop {
            match self.rx.try_recv() {
                Ok(RxEventSignal::Done) => return Ok(()),
                Err(TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(TryRecvError::Disconnected) => {
                    return Err(anyhow::anyhow!("BatchProcessor worker thread disconnected"))
                }
            }
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        let join_handle = match self.join_handle.take() {
            Some(handle) => handle,
            None => return Ok(()), // Already stopped
        };

        self.tx.send(TxEventSignal::Stop)?;
        match join_handle.join() {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Error stopping BatchProcessor: {:?}", e)),
        }
    }

    pub fn restart(&mut self, api_config: APIWrapper, max_batch_size: usize) -> Result<()> {
        self.stop()?;
        let (tx, rx, join_handle) = start_worker(&api_config, max_batch_size);
        self.api_config = api_config;
        self.tx = tx;
        self.rx = rx;
        self.join_handle = Some(join_handle);
        Ok(())
    }
}
