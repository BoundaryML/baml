use sys_types::{CallId, CancellationToken};

/// Per-call context passed to [`crate::BexEngine::call_function`].
///
/// Constructed via [`FunctionCallContextBuilder`].
pub struct FunctionCallContext {
    pub call_id: CallId,
    pub cancel: CancellationToken,
    /// Type arguments to seed the entry frame's `type_args` slot with.
    /// Use to call generic stdlib functions like `baml.json.to_string<T>`
    /// from a host: the native handler reads its `T` from this channel.
    pub type_args: Vec<baml_type::RuntimeTy>,
}

/// Builder for `FunctionCallContext`.
pub struct FunctionCallContextBuilder {
    call_id: CallId,
    cancel: Option<CancellationToken>,
    type_args: Option<Vec<baml_type::RuntimeTy>>,
}

impl FunctionCallContextBuilder {
    pub fn new(call_id: CallId) -> Self {
        Self {
            call_id,
            cancel: None,
            type_args: None,
        }
    }

    #[must_use]
    pub fn build(self) -> FunctionCallContext {
        FunctionCallContext {
            call_id: self.call_id,
            cancel: self.cancel.unwrap_or_default(),
            type_args: self.type_args.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn with_type_args(mut self, type_args: Vec<baml_type::RuntimeTy>) -> Self {
        self.type_args = Some(type_args);
        self
    }

    #[must_use]
    pub fn with_cancel_token(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }
}
