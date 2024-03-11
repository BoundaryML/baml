use anyhow::Result;
mod custom_exporter;
pub(super) mod event_thread;
mod partial_types;
pub(super) mod span_events;
// pub mod tracer;

use tracing_subscriber::prelude::*;

use crate::api_wrapper::{core_types::TestCaseStatus, APIWrapper, BoundaryAPI, BoundaryTestAPI};

use self::{event_thread::BatchProcessor, span_events::BamlEventSubscriber};

static mut DEFAULT_HANDLER: Option<BatchProcessor> = None;

fn maybe_create_config() -> bool {
  unsafe {
    match &DEFAULT_HANDLER {
      Some(_) => false,
      None => {
        let processor = BatchProcessor::new(APIWrapper::default(), 100);
        DEFAULT_HANDLER = Some(processor);
        let subscriber = tracing_subscriber::registry::Registry::default()
          .with(BamlEventSubscriber::new(DEFAULT_HANDLER.as_mut().unwrap()));
        tracing::subscriber::set_global_default(subscriber).unwrap();
        true
      }
    }
  }
}

pub fn event_handler() -> Option<&'static mut BatchProcessor> {
  unsafe {
    match &mut DEFAULT_HANDLER {
      Some(processor) => Some(processor),
      None => None,
    }
  }
}

pub fn init_tracer() {
  maybe_create_config();
}

pub fn stop_tracer() -> Result<()> {
  if let Some(config) = event_handler() {
    config.stop()
  } else {
    Ok(())
  }
}

pub fn flush_tracer() -> Result<()> {
  if let Some(config) = event_handler() {
    config.flush()
  } else {
    Ok(())
  }
}

pub fn log_event(name: span_events::SpanEvent, raw_content: serde_json::Value) -> Result<()> {
  if let Some(_) = event_handler() {
    span_events::log_event(name, raw_content)
  } else {
    Ok(())
  }
}

// pub(super) async fn update_tracer(
//     base_url: Option<&str>,
//     api_key: Option<&str>,
//     project_id: Option<&str>,
//     sessions_id: Option<&str>,
//     stage: Option<&str>,
// ) {
//     // Update the default config
//     let config = DEFAULT_HANDLER().copy_from(base_url, api_key, project_id, sessions_id, stage);
//     unsafe {
//         DEFAULT_HANDLER = Some(config.clone());
//     }

//     // Update the exporter
//     set_exporter(CustomBackendExporter::new(config)).await;
// }
