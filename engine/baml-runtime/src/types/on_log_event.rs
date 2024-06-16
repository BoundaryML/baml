use anyhow::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEvent {
    pub metadata: LogEventMetadata,
    pub prompt: Option<String>,
    pub raw_output: Option<String>,
    // json structure or a string
    pub parsed_output: Option<String>,
    pub start_time: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]

pub struct LogEventMetadata {
    pub event_id: String,
    pub parent_id: Option<String>,
    pub root_event_id: String,
}

pub type LogEventCallbackSync = Box<dyn Fn(LogEvent) -> Result<(), Error> + Send + Sync>;
