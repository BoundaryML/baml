use bex_events::ids::BoundaryId;
use indexmap::IndexMap;
use sys_types::{CallId, CancellationToken};

use crate::value_capture::TraceCaptureProducer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryContext {
    pub boundary_id: BoundaryId,
    pub capture_defaults: CaptureDefaults,
    pub storage_context: BoundaryStorageContext,
    /// Optional exact-evidence trigger armed by the host for this boundary.
    pub manual_trigger: Option<String>,
    /// Root-boundary latency trigger. Per-call latency aggregates remain in
    /// the CCT; this threshold requests an exact flight dump on completion.
    pub latency_trigger_ms: Option<u64>,
}

impl BoundaryContext {
    #[must_use]
    pub fn new(boundary_id: BoundaryId) -> Self {
        Self {
            boundary_id,
            capture_defaults: CaptureDefaults::disabled(),
            storage_context: BoundaryStorageContext::default(),
            manual_trigger: None,
            latency_trigger_ms: bex_events::prof::ProfConfig::global().latency_trigger_ms,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureDefaults {
    pub values_enabled: bool,
    pub logs_enabled: bool,
}

impl CaptureDefaults {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            values_enabled: false,
            logs_enabled: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryStorageContext {
    /// Project root that owns `.baml/history`. `None` resolves from the
    /// process working directory at the cold boundary-begin path.
    pub project_root: Option<std::path::PathBuf>,
    /// Stable host label (`cli`, `sdk`, `playground`, ...).
    pub source: String,
    /// Optional exported project identity.
    pub project_id: String,
    /// Per-project durable history policy. Profiling may remain active in
    /// session-only mode when this is false, matching `BAML_HISTORY=0`.
    pub durable_history_enabled: bool,
    /// Host-level value/log defaults, normally sourced from
    /// `baml.toml [observability]`.
    pub capture_values: bool,
    pub capture_logs: bool,
    /// A project override for the exact-evidence latency trigger.
    pub latency_trigger_ms: Option<u64>,
}

impl Default for BoundaryStorageContext {
    fn default() -> Self {
        Self {
            project_root: None,
            source: "sdk".to_owned(),
            project_id: String::new(),
            durable_history_enabled: true,
            capture_values: true,
            capture_logs: false,
            latency_trigger_ms: None,
        }
    }
}

impl BoundaryStorageContext {
    #[must_use]
    pub fn new(source: impl Into<String>, project_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            project_root: Some(project_root.into()),
            source: source.into(),
            project_id: String::new(),
            durable_history_enabled: true,
            capture_values: true,
            capture_logs: false,
            latency_trigger_ms: None,
        }
    }

    #[must_use]
    pub fn with_project_id(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = project_id.into();
        self
    }

    #[must_use]
    pub fn with_observability(
        mut self,
        durable_history_enabled: bool,
        capture_values: bool,
        capture_logs: bool,
        latency_trigger_ms: Option<u64>,
    ) -> Self {
        self.durable_history_enabled = durable_history_enabled;
        self.capture_values = capture_values;
        self.capture_logs = capture_logs;
        self.latency_trigger_ms = latency_trigger_ms.filter(|threshold| *threshold != 0);
        self
    }
}

/// Per-call context passed to [`crate::BexEngine::call_function`].
///
/// Constructed via [`FunctionCallContextBuilder`].
pub struct FunctionCallContext {
    pub host_call_id: CallId,
    pub boundary: BoundaryContext,
    pub value_capture: TraceCaptureProducer,
    pub cancel: CancellationToken,
    pub profile_enabled: bool,
    /// Named `TypeVar` bindings for a generic call. Each entry is
    /// `TypeVar name -> concrete type`; insertion order is the callee's De
    /// Bruijn order. Sourced from a host SDK call (`CallFunctionArgs.type_args`)
    /// or from internal Rust callers invoking generic stdlib functions like
    /// `baml.json.to_string<T>` (which bind their `T` by name here). The engine
    /// lowers these to the positional `type_args` slot by matching names against
    /// the callee's generic params in `set_entry_point_with_type_args`.
    /// Empty for non-generic / internal calls.
    pub type_args: IndexMap<String, baml_type::RuntimeTy>,
}

/// Builder for `FunctionCallContext`.
pub struct FunctionCallContextBuilder {
    host_call_id: CallId,
    boundary: BoundaryContext,
    value_capture: TraceCaptureProducer,
    cancel: Option<CancellationToken>,
    profile_enabled: bool,
    type_args: Option<IndexMap<String, baml_type::RuntimeTy>>,
}

impl FunctionCallContextBuilder {
    /// Bounded host defaults for root input/output/error capture and
    /// function-level `Auto` capture (notably LLM calls). One producer is
    /// allocated per root invocation and error paths are promoted by the
    /// boundary lifecycle.
    pub const DEFAULT_PENDING_VALUE_DRAFTS: usize = 4_096;
    pub const DEFAULT_PENDING_LOG_DRAFTS: usize = 4_096;

    pub fn new(host_call_id: CallId) -> Self {
        Self {
            host_call_id,
            boundary: BoundaryContext::new(BoundaryId::new_random()),
            value_capture: TraceCaptureProducer::disabled(),
            cancel: None,
            profile_enabled: true,
            type_args: None,
        }
    }

    #[must_use]
    pub fn build(self) -> FunctionCallContext {
        FunctionCallContext {
            host_call_id: self.host_call_id,
            boundary: self.boundary,
            value_capture: self.value_capture,
            cancel: self.cancel.unwrap_or_default(),
            profile_enabled: self.profile_enabled,
            type_args: self.type_args.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn with_boundary_id(mut self, boundary_id: BoundaryId) -> Self {
        self.boundary.boundary_id = boundary_id;
        self
    }

    #[must_use]
    pub fn with_capture_defaults(mut self, capture_defaults: CaptureDefaults) -> Self {
        self.boundary.capture_defaults = capture_defaults;
        self
    }

    /// Apply the normal host capture policy without bypassing the durable
    /// history/profile opt-outs. When history is disabled the context remains
    /// capture-free and no value producer work is forced.
    #[must_use]
    pub fn with_default_history_capture(self, logs_enabled: bool) -> Self {
        let enabled = bex_events::prof::history_enabled()
            && bex_events::prof::ProfConfig::global().is_enabled()
            && self.boundary.storage_context.durable_history_enabled;
        let capture_values = self.boundary.storage_context.capture_values;
        let capture_logs = logs_enabled || self.boundary.storage_context.capture_logs;
        self.with_history_capture_state(enabled, capture_values, capture_logs)
    }

    fn with_history_capture_state(
        mut self,
        enabled: bool,
        capture_values: bool,
        logs_enabled: bool,
    ) -> Self {
        if enabled {
            self.boundary.capture_defaults = CaptureDefaults {
                values_enabled: capture_values,
                logs_enabled,
            };
            self.value_capture = TraceCaptureProducer::new(
                crate::value_capture::TraceCaptureConfig::enabled_with_budgets(
                    if capture_values {
                        Self::DEFAULT_PENDING_VALUE_DRAFTS
                    } else {
                        0
                    },
                    if logs_enabled {
                        Self::DEFAULT_PENDING_LOG_DRAFTS
                    } else {
                        0
                    },
                ),
            );
        }
        self
    }

    #[must_use]
    pub fn with_boundary_storage(mut self, storage_context: BoundaryStorageContext) -> Self {
        if let Some(threshold_ms) = storage_context.latency_trigger_ms {
            self.boundary.latency_trigger_ms = Some(threshold_ms);
        }
        self.boundary.storage_context = storage_context;
        self
    }

    #[must_use]
    pub fn with_manual_trigger(mut self, label: impl Into<String>) -> Self {
        self.boundary.manual_trigger = Some(label.into());
        self
    }

    #[must_use]
    pub fn with_latency_trigger_ms(mut self, threshold_ms: u64) -> Self {
        self.boundary.latency_trigger_ms = (threshold_ms != 0).then_some(threshold_ms);
        self
    }

    #[must_use]
    pub fn with_value_capture(mut self, value_capture: TraceCaptureProducer) -> Self {
        self.value_capture = value_capture;
        self
    }

    /// Seed named `TypeVar` bindings for a generic call. Insertion order should
    /// be the callee's De Bruijn order. The engine resolves them to positional
    /// slots against the callee's generic params.
    #[must_use]
    pub fn with_type_args(mut self, type_args: IndexMap<String, baml_type::RuntimeTy>) -> Self {
        self.type_args = Some(type_args);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_capture_policy_is_bounded_and_opt_out_preserving() {
        let disabled = FunctionCallContextBuilder::new(CallId::next())
            .with_history_capture_state(false, true, true)
            .build();
        assert_eq!(
            disabled.boundary.capture_defaults,
            CaptureDefaults::disabled()
        );

        let enabled = FunctionCallContextBuilder::new(CallId::next())
            .with_boundary_storage(BoundaryStorageContext::new("test", "/tmp/project"))
            .with_history_capture_state(true, true, false)
            .build();
        assert!(enabled.boundary.capture_defaults.values_enabled);
        assert!(!enabled.boundary.capture_defaults.logs_enabled);
        assert_eq!(enabled.boundary.storage_context.source, "test");
        assert_eq!(
            enabled.boundary.storage_context.project_root.as_deref(),
            Some(std::path::Path::new("/tmp/project"))
        );
    }
}
