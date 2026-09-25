//! Function policy configuration and immutable policy-table sizing.
/// **Measure first for policy lookup.** Trades page allocation granularity against table
/// shape; must divide the ID space. Policy-zero calls bypass this table.
pub const PAGE_SIZE: usize = 256;
/// **Representation limit.** Derived from the policy ID width; zero denotes no policy.
pub const POLICY_SLOTS: usize = u16::MAX as usize + 1;
/// **Derived.** Calculated from ID space and page size; never tune independently.
pub const PAGE_COUNT: usize = POLICY_SLOTS / PAGE_SIZE;
const _: () = assert!(PAGE_SIZE > 0 && POLICY_SLOTS.is_multiple_of(PAGE_SIZE));

/// Threshold is resolved by the clock owner; settings have no dependency on
/// clock implementation or VM types.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent resolved capture requirements"
)]
pub struct TelemetryPolicy<Threshold> {
    pub span_from_entry: bool,
    /// Duration threshold resolved to the invocation clock domain, not a wall timestamp.
    pub promotion_duration_threshold: Option<Threshold>,
    pub promote_errors: bool,
    pub capture_inputs: bool,
    pub capture_output: bool,
    pub capture_error: bool,
}
impl<T> TelemetryPolicy<T> {
    pub const NONE: Self = Self {
        span_from_entry: false,
        promotion_duration_threshold: None,
        promote_errors: false,
        capture_inputs: false,
        capture_output: false,
        capture_error: false,
    };
}
/// **Observability semantics.** Disabling entry input capture changes the data users receive;
/// it is not equivalent-work optimization.
pub const AI_CAPTURE_INPUTS: bool = true;
/// **Observability semantics.** Sticky output capture for AI invocations; change only with
/// the capture contract.
pub const AI_CAPTURE_OUTPUT: bool = true;
/// **Observability semantics.** Sticky error capture for AI invocations; change only with the
/// capture contract.
pub const AI_CAPTURE_ERROR: bool = true;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationMode {
    Hidden,
    Timing,
    Span,
}

/// **Observability semantics.** Hidden removes timing/promotion eligibility for calls
/// entering that mode; Span adds identity work. Preserve intended observations.
pub const BYTECODE_DEFAULT_MODE: InvocationMode = InvocationMode::Timing;
/// **Fixed support boundary.** Native instrumentation is currently unsupported and must
/// remain Hidden until implemented.
pub const NATIVE_DEFAULT_MODE: InvocationMode = InvocationMode::Hidden;
/// **Observability semantics.** AI calls are identified from entry; changing mode affects
/// announcements and ancestry.
pub const AI_DEFAULT_MODE: InvocationMode = InvocationMode::Span;
/// **Fixed implementation.** The VM currently implements one entry. A larger cache needs a
/// resolver design and nested/alternating-call benchmarks.
pub const CALL_PATH_CACHE_SLOTS: usize = 1;
