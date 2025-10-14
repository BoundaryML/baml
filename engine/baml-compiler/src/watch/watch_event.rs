use std::fmt;

use baml_types::{BamlValueWithMeta, Completion, Constraint, ResponseCheck, TypeIR};

/// Unique identifier for a streaming emit event
pub type StreamId = String;

#[derive(Debug)]
pub enum EmitBamlValue {
    Value(BamlValueWithMeta<EmitValueMetadata>),
    Block(String),
    StreamStart(StreamId),
    StreamUpdate(StreamId, BamlValueWithMeta<EmitValueMetadata>),
    StreamEnd(StreamId),
}

/// The BamlValueWithMeta metadata for a
/// BamlValue in an event.
#[derive(Debug, Clone)]
pub struct EmitValueMetadata {
    pub constraints: Vec<Constraint>,
    pub response_checks: Vec<ResponseCheck>,
    pub completion: Completion,
    pub r#type: TypeIR,
}

#[derive(Debug)]
pub struct EmitEvent {
    pub value: EmitBamlValue,
    pub variable_name: Option<String>,
    pub function_name: String,
    pub is_stream: bool,
}

impl fmt::Display for EmitEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.value {
            EmitBamlValue::Value(value) => {
                if let Some(var_name) = &self.variable_name {
                    write!(f, "(var) {}: {}", var_name, value.clone().value())
                } else {
                    write!(f, "{}", value.clone().value())
                }
            }
            EmitBamlValue::Block(label) => {
                write!(f, "(block) {}", label)
            }
            EmitBamlValue::StreamStart(stream_id) => {
                write!(f, "(stream start) {}", stream_id)
            }
            EmitBamlValue::StreamUpdate(stream_id, value) => {
                write!(
                    f,
                    "(stream update) {}: {}",
                    stream_id,
                    value.clone().value()
                )
            }
            EmitBamlValue::StreamEnd(stream_id) => {
                write!(f, "(stream end) {}", stream_id)
            }
        }
    }
}

impl EmitEvent {
    pub fn new_var(
        variable_name: String,
        value: BamlValueWithMeta<EmitValueMetadata>,
        function_name: String,
    ) -> Self {
        Self {
            value: EmitBamlValue::Value(value),
            variable_name: Some(variable_name),
            function_name,
            is_stream: false,
        }
    }

    pub fn new_stream(
        variable_name: String,
        value: BamlValueWithMeta<EmitValueMetadata>,
        function_name: String,
    ) -> Self {
        Self {
            value: EmitBamlValue::Value(value),
            variable_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }

    pub fn new_block(block_label: String, function_name: String) -> Self {
        Self {
            value: EmitBamlValue::Block(block_label),
            variable_name: None,
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
            value: EmitBamlValue::StreamStart(stream_id),
            variable_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }

    pub fn new_stream_update(
        variable_name: String,
        stream_id: StreamId,
        value: BamlValueWithMeta<EmitValueMetadata>,
        function_name: String,
    ) -> Self {
        Self {
            value: EmitBamlValue::StreamUpdate(stream_id, value),
            variable_name: Some(variable_name),
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
            value: EmitBamlValue::StreamEnd(stream_id),
            variable_name: Some(variable_name),
            function_name,
            is_stream: true,
        }
    }
}
