use std::sync::Arc;

use bex_events::HostSpanContext;
use sys_types::{CallId, CancellationToken};

/// Per-call context passed to [`crate::BexEngine::call_function`].
///
/// Constructed via [`FunctionCallContextBuilder`].
pub struct FunctionCallContext {
    pub call_id: CallId,
    pub host_ctx: Option<HostSpanContext>,
    pub collectors: Vec<Arc<bex_events::Collector>>,
    pub cancel: CancellationToken,
    /// Type arguments to seed the entry frame's `type_args` slot with.
    /// Use to call generic stdlib functions like `baml.json.to_string<T>`
    /// from a host: the native handler reads its `T` from this channel.
    pub type_args: Vec<baml_type::Ty>,
}

/// Builder for `FunctionCallContext`.
pub struct FunctionCallContextBuilder {
    call_id: CallId,
    host_ctx: Option<bex_events::HostSpanContext>,
    collectors: Option<Vec<Arc<bex_events::Collector>>>,
    cancel: Option<CancellationToken>,
    type_args: Option<Vec<baml_type::Ty>>,
}

impl FunctionCallContextBuilder {
    pub fn new(call_id: CallId) -> Self {
        Self {
            call_id,
            host_ctx: None,
            collectors: None,
            cancel: None,
            type_args: None,
        }
    }

    #[must_use]
    pub fn build(self) -> FunctionCallContext {
        FunctionCallContext {
            call_id: self.call_id,
            host_ctx: self.host_ctx,
            collectors: self.collectors.unwrap_or_default(),
            cancel: self.cancel.unwrap_or_default(),
            type_args: self.type_args.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn with_type_args(mut self, type_args: Vec<baml_type::Ty>) -> Self {
        self.type_args = Some(type_args);
        self
    }

    #[must_use]
    pub fn with_host_ctx(mut self, host_ctx: bex_events::HostSpanContext) -> Self {
        self.host_ctx = Some(host_ctx);
        self
    }

    #[must_use]
    pub fn with_collectors(mut self, collectors: Vec<Arc<bex_events::Collector>>) -> Self {
        self.collectors = Some(collectors);
        self
    }

    #[must_use]
    pub fn with_cancel_token(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }
}
