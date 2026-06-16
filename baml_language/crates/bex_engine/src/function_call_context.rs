use sys_types::{CallId, CancellationToken};

/// Per-call context passed to [`crate::BexEngine::call_function`].
///
/// Constructed via [`FunctionCallContextBuilder`].
pub struct FunctionCallContext {
    pub host_call_id: CallId,
    pub cancel: CancellationToken,
    pub profile_enabled: bool,
    /// Positional type arguments to seed the entry frame's `type_args` slot
    /// with, in De Bruijn order. Use to call generic stdlib functions like
    /// `baml.json.to_string<T>` from internal Rust callers: the native handler
    /// reads its `T` from this channel.
    pub type_args: Vec<baml_type::RuntimeTy>,
    /// Named `TypeVar` bindings from a host SDK call (`CallFunctionArgs.type_args`).
    /// Each pair is `(TypeVar name, concrete type)`. The engine lowers these to
    /// the positional `type_args` slot by matching names against the callee's
    /// generic params in `set_entry_point_with_type_args`. Takes precedence over
    /// `type_args` when non-empty. Empty for non-generic / internal calls.
    pub named_type_args: Vec<(String, baml_type::RuntimeTy)>,
}

/// Builder for `FunctionCallContext`.
pub struct FunctionCallContextBuilder {
    host_call_id: CallId,
    cancel: Option<CancellationToken>,
    profile_enabled: bool,
    type_args: Option<Vec<baml_type::RuntimeTy>>,
    named_type_args: Option<Vec<(String, baml_type::RuntimeTy)>>,
}

impl FunctionCallContextBuilder {
    pub fn new(host_call_id: CallId) -> Self {
        Self {
            host_call_id,
            cancel: None,
            profile_enabled: true,
            type_args: None,
            named_type_args: None,
        }
    }

    #[must_use]
    pub fn build(self) -> FunctionCallContext {
        FunctionCallContext {
            host_call_id: self.host_call_id,
            cancel: self.cancel.unwrap_or_default(),
            profile_enabled: self.profile_enabled,
            type_args: self.type_args.unwrap_or_default(),
            named_type_args: self.named_type_args.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn with_type_args(mut self, type_args: Vec<baml_type::RuntimeTy>) -> Self {
        self.type_args = Some(type_args);
        self
    }

    /// Seed named `TypeVar` bindings from a host SDK call. The engine resolves
    /// them to positional De Bruijn slots against the callee's generic params.
    #[must_use]
    pub fn with_named_type_args(
        mut self,
        named_type_args: Vec<(String, baml_type::RuntimeTy)>,
    ) -> Self {
        self.named_type_args = Some(named_type_args);
        self
    }

    #[must_use]
    pub fn with_cancel_token(mut self, cancel: CancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    #[must_use]
    pub fn with_profile_enabled(mut self, enabled: bool) -> Self {
        self.profile_enabled = enabled;
        self
    }
}
