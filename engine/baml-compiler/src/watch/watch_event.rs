use std::{
    fmt,
    sync::{Arc, Mutex},
};

use baml_types::{BamlValueWithMeta, Completion, Constraint, ResponseCheck, TypeIR};
pub use baml_viz_events::{RuntimeNodeType, VizExecDelta, VizExecEvent};
use baml_viz_events::{StateUpdate, VizStateReducer};

/// Unique identifier for a streaming watch notification
pub type StreamId = String;

/// Concrete handler for watch notifications.
/// This replaces the trait-based approach with conditional Send bounds.
/// Use Arc<Mutex<WatchHandler>> for shared ownership across threads.
pub struct WatchHandler<F>
where
    F: FnMut(WatchNotification),
{
    handler: F,
}

impl<F> WatchHandler<F>
where
    F: FnMut(WatchNotification),
{
    pub fn new(handler: F) -> Self {
        WatchHandler { handler }
    }

    pub fn notify(&mut self, notification: WatchNotification) {
        (self.handler)(notification)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WatchHandler<Box<dyn FnMut(WatchNotification) + Send>> {
    /// Create a no-op handler (native)
    pub fn noop() -> Self {
        WatchHandler {
            handler: Box::new(|_| {}),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl WatchHandler<Box<dyn FnMut(WatchNotification)>> {
    /// Create a no-op handler (WASM)
    pub fn noop() -> Self {
        WatchHandler {
            handler: Box::new(|_| {}),
        }
    }
}

/// Type alias for shared watch handler (thread-safe for native, single-threaded for WASM)
#[cfg(not(target_arch = "wasm32"))]
pub type SharedWatchHandler = Arc<Mutex<WatchHandler<Box<dyn FnMut(WatchNotification) + Send>>>>;

#[cfg(target_arch = "wasm32")]
pub type SharedWatchHandler = Arc<Mutex<WatchHandler<Box<dyn FnMut(WatchNotification)>>>>;

/// Helper to create a shared watch handler from a function (native - requires Send)
#[cfg(not(target_arch = "wasm32"))]
pub fn shared_handler<F>(handler: F) -> SharedWatchHandler
where
    F: FnMut(WatchNotification) + Send + 'static,
{
    Arc::new(Mutex::new(WatchHandler::new(Box::new(handler))))
}

/// Helper to create a shared watch handler from a function (WASM - no Send requirement)
#[cfg(target_arch = "wasm32")]
pub fn shared_handler<F>(handler: F) -> SharedWatchHandler
where
    F: FnMut(WatchNotification) + 'static,
{
    #[allow(clippy::arc_with_non_send_sync)]
    Arc::new(Mutex::new(WatchHandler::new(Box::new(handler))))
}

/// Helper to create a shared no-op handler
pub fn shared_noop_handler() -> SharedWatchHandler {
    #[allow(clippy::arc_with_non_send_sync)]
    Arc::new(Mutex::new(WatchHandler::noop()))
}

#[derive(Debug, Clone)]
pub enum WatchBamlValue {
    Value(BamlValueWithMeta<WatchValueMetadata>),
    VizExecState(VizExecEvent),
    StreamStart(StreamId),
    StreamUpdate(StreamId, BamlValueWithMeta<WatchValueMetadata>),
    StreamEnd(StreamId),
}

#[derive(Debug, Clone)]
pub enum ReducedWatchBamlValue {
    Value(BamlValueWithMeta<WatchValueMetadata>),
    VizStateUpdate(StateUpdate),
    StreamStart(StreamId),
    StreamUpdate(StreamId, BamlValueWithMeta<WatchValueMetadata>),
    StreamEnd(StreamId),
}

pub struct WatchEventReducer(pub VizStateReducer);

impl Default for WatchEventReducer {
    fn default() -> Self {
        Self::new()
    }
}

impl WatchEventReducer {
    pub fn new() -> Self {
        Self(VizStateReducer::default())
    }

    pub fn apply(
        &mut self,
        function_name: &str,
        watch_event: WatchBamlValue,
    ) -> Vec<ReducedWatchBamlValue> {
        match watch_event {
            WatchBamlValue::VizExecState(event) => self
                .0
                .apply(function_name, &event)
                .into_iter()
                .map(ReducedWatchBamlValue::VizStateUpdate)
                .collect(),
            WatchBamlValue::Value(value) => vec![ReducedWatchBamlValue::Value(value.clone())],
            WatchBamlValue::StreamStart(stream_id) => {
                vec![ReducedWatchBamlValue::StreamStart(stream_id.clone())]
            }
            WatchBamlValue::StreamUpdate(stream_id, value) => {
                vec![ReducedWatchBamlValue::StreamUpdate(
                    stream_id.clone(),
                    value.clone(),
                )]
            }
            WatchBamlValue::StreamEnd(stream_id) => {
                vec![ReducedWatchBamlValue::StreamEnd(stream_id.clone())]
            }
        }
    }
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
            WatchBamlValue::VizExecState(event) => {
                let delta = match event.event {
                    VizExecDelta::Enter => "enter",
                    VizExecDelta::Exit => "exit",
                };
                write!(
                    f,
                    "(context {delta}) n{node_id} {segment}",
                    node_id = event.node_id,
                    segment = event.path_segment.encode()
                )
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

    pub fn new_viz_exec_state(event: VizExecEvent, function_name: String) -> Self {
        Self {
            value: WatchBamlValue::VizExecState(event),
            variable_name: None,
            channel_name: None,
            function_name,
            is_stream: false,
        }
    }
}
