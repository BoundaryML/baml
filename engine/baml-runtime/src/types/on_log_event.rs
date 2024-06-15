use anyhow::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEvent {
    id: String,
}

type LogEventCallback = Option<Box<dyn Fn(LogEvent) -> Result<(), Error> + Send>>;
