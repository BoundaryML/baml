use anyhow::Result;
use napi::tokio::runtime::Runtime;
use std::{
  sync::mpsc::{Receiver, Sender, TryRecvError},
  time::Duration,
};

use crate::api_wrapper::{api_interface::BoundaryAPI, core_types::LogSchema, APIWrapper};

async fn process_batch_async(api_config: &APIWrapper, batch: Vec<LogSchema>) {
  for work in batch {
    api_config.pretty_print(&work);
    let _ = api_config.log_schema(&work).await;
  }
}

fn process_batch(rt: &Runtime, api_config: &APIWrapper, batch: Vec<LogSchema>) {
  println!("Processing batch of {} logs", batch.len());
  rt.block_on(process_batch_async(api_config, batch));
}

fn batch_processor(
  api_config: APIWrapper,
  rx: Receiver<TxEventSignal>,
  tx: Sender<RxEventSignal>,
  max_batch_size: usize,
) {
  let api_config = &api_config;
  let mut batch = Vec::with_capacity(if api_config.is_test_mode() {
    1
  } else {
    max_batch_size
  });
  let mut now = std::time::Instant::now();
  let rt = Runtime::new().unwrap();
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

    let time_trigger = if now.elapsed().as_millis() >= 1000 {
      true
    } else {
      false
    };

    let should_process_batch = (batch_full || flush || exit || time_trigger) && !batch.is_empty();

    // Send events every 1 second or when the batch is full
    if should_process_batch {
      println!(
        "Trigger hit: {} or {}ms",
        batch.len(),
        now.elapsed().as_millis()
      );
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

enum TxEventSignal {
  Stop,
  Flush,
  Submit(LogSchema),
}

enum RxEventSignal {
  Done,
}

pub struct BatchProcessor {
  api_config: APIWrapper,
  tx: std::sync::Arc<std::sync::Mutex<Sender<TxEventSignal>>>,
  rx: std::sync::Arc<std::sync::Mutex<Receiver<RxEventSignal>>>,
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
      tx: std::sync::Arc::new(std::sync::Mutex::new(tx)),
      rx: std::sync::Arc::new(std::sync::Mutex::new(rx)),
      join_handle: Some(join_handle),
    }
  }

  pub fn submit(&self, work: LogSchema) -> Result<()> {
    let tx = match self.tx.lock() {
      Ok(tx) => tx,
      Err(e) => return Err(anyhow::anyhow!("Error submitting work: {:?}", e)),
    };
    tx.send(TxEventSignal::Submit(work))?;
    Ok(())
  }

  pub fn flush(&self) -> Result<()> {
    // Send a flush signal to the worker
    self
      .tx
      .lock()
      .map_err(|e| anyhow::anyhow!("Error flushing BatchProcessor: {:?}", e))?
      .send(TxEventSignal::Flush)?;

    // Wait for the worker to finish processing the flush
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

  pub fn stop(&mut self) -> Result<()> {
    println!("Stopping batch processor");
    let join_handle = match self.join_handle.take() {
      Some(handle) => handle,
      None => return Ok(()), // Already stopped
    };

    println!("Sending stop signal");
    self
      .tx
      .lock()
      .map_err(|e| anyhow::anyhow!("Error stopping BatchProcessor: {:?}", e))?
      .send(TxEventSignal::Stop)?;
    match join_handle.join() {
      Ok(_) => Ok(()),
      Err(e) => Err(anyhow::anyhow!("Error stopping BatchProcessor: {:?}", e)),
    }
  }

  pub fn restart(&mut self, api_config: APIWrapper, max_batch_size: usize) -> Result<()> {
    self.stop()?;
    let (tx, rx, join_handle) = start_worker(&api_config, max_batch_size);
    self.api_config = api_config;
    self.tx = std::sync::Arc::new(std::sync::Mutex::new(tx));
    self.rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
    self.join_handle = Some(join_handle);
    Ok(())
  }
}

impl Drop for BatchProcessor {
  fn drop(&mut self) {
    println!("Dropping batch processor");
    match self.stop() {
      Ok(_) => {
        println!("Done! Stopped batch processor");
      }
      Err(e) => println!("Error stopping BatchProcessor: {:?}", e),
    }
  }
}
