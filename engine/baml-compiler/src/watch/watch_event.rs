use std::fmt;

use baml_types::{BamlValueWithMeta, Completion, Constraint, ResponseCheck, TypeIR};

/// Unique identifier for a streaming watch notification
pub type StreamId = String;

#[derive(Debug)]
pub enum WatchBamlValue {
    Value(BamlValueWithMeta<WatchValueMetadata>),
    Block(String),
    StreamStart(StreamId),
    StreamUpdate(StreamId, BamlValueWithMeta<WatchValueMetadata>),
    StreamEnd(StreamId),
}

/// The BamlValueWithMeta metadata for a
/// BamlValue in a notification.
#[derive(Debug, Clone)]
pub struct WatchValueMetadata {
    pub constraints: Vec<Constraint>,
    pub response_checks: Vec<ResponseCheck>,
    pub completion: Completion,
    pub r#type: TypeIR,
}

#[derive(Debug)]
pub struct WatchNotification {
    pub value: WatchBamlValue,
    pub variable_name: Option<String>,
    pub channel_name: Option<String>,
    pub function_name: String,
    pub is_stream: bool,
}

impl fmt::Display for WatchNotification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            WatchBamlValue::Value(value) => match (&self.variable_name, &self.channel_name) {
                (Some(var_name), Some(chan_name)) if var_name != chan_name => {
                    write!(
                        f,
                        "(var) {} [channel: {}]: {}",
                        var_name,
                        chan_name,
                        value.clone().value()
                    )
                }
                (Some(var_name), _) => {
                    write!(f, "(var) {}: {}", var_name, value.clone().value())
                }
                (None, Some(chan_name)) => {
                    write!(f, "(channel) {}: {}", chan_name, value.clone().value())
                }
                _ => {
                    write!(f, "{}", value.clone().value())
                }
            },
            WatchBamlValue::Block(label) => {
                write!(f, "(block) {label}")
            }
            WatchBamlValue::StreamStart(stream_id) => {
                write!(f, "(stream start) {stream_id}")
            }
            WatchBamlValue::StreamUpdate(stream_id, value) => {
                write!(
                    f,
                    "(stream update) {}: {}",
                    stream_id,
                    value.clone().value()
                )
            }
            WatchBamlValue::StreamEnd(stream_id) => {
                write!(f, "(stream end) {stream_id}")
            }
        }
    }
}

impl WatchNotification {
    pub fn new_var(
        variable_name: String,
        channel_name: String,
        value: BamlValueWithMeta<WatchValueMetadata>,
        function_name: String,
    ) -> Self {
        Self {
            value: WatchBamlValue::Value(value),
            variable_name: Some(variable_name),
            channel_name: Some(channel_name),
            function_name,
            is_stream: false,
        }
    }

    pub fn new_stream(
        variable_name: String,
        value: BamlValueWithMeta<WatchValueMetadata>,
        function_name: String,
    ) -> Self {
        Self {
            value: WatchBamlValue::Value(value),
            variable_name: Some(variable_name.clone()),
            channel_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }

    pub fn new_block(block_label: String, function_name: String) -> Self {
        Self {
            value: WatchBamlValue::Block(block_label),
            variable_name: None,
            channel_name: None,
            function_name,
            is_stream: false,
        }
    }

    pub fn new_stream_start(
        variable_name: String,
        stream_id: StreamId,
        function_name: String,
    ) -> Self {
        Self {
            value: WatchBamlValue::StreamStart(stream_id),
            variable_name: Some(variable_name.clone()),
            channel_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }

    pub fn new_stream_update(
        variable_name: String,
        stream_id: StreamId,
        value: BamlValueWithMeta<WatchValueMetadata>,
        function_name: String,
    ) -> Self {
        Self {
            value: WatchBamlValue::StreamUpdate(stream_id, value),
            variable_name: Some(variable_name.clone()),
            channel_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }

    pub fn new_stream_end(
        variable_name: String,
        stream_id: StreamId,
        function_name: String,
    ) -> Self {
        Self {
            value: WatchBamlValue::StreamEnd(stream_id),
            variable_name: Some(variable_name.clone()),
            channel_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }
}
