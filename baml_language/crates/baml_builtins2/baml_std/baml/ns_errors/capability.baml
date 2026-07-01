// Capability error model for the provider/LLM interfaces (see _plan/llm-provider-plan.md,
// decisions D2/D8). Two pieces:
//
//   1. `UnknownError` — the universal escape hatch. Any foreign thrown value boxes
//      into it (data preserved untouched, plus a breadcrumb of context), so a
//      fallible method never fails to typecheck because a dependency threw something
//      exotic. The channel every capability method declares is `<CapError> | UnknownError`.
//
//   2. One classifier `interface` per capability (flat, NOT a hierarchy). A concrete
//      provider error `implements` whichever apply, so a caller can triage a failure
//      without knowing the concrete class.
//
// NOTE: the literal `UnknownError.from<T>` / `with_message<T>` "reassert the channel"
// helpers from the design are intentionally omitted — they require a runtime
// "is this value a (monomorphized) T?" test, which the language does not currently
// support (`v is T` on a generic parameter is always false; `match { let t: T => }`
// is an irrefutable catch-all). Foreign errors are instead normalized at the `catch`
// site via an interface-match arm:
//     catch (e) { let c: root.errors.CallError => throw c, _ => throw UnknownError { ... } }

/// The universal wrapper: a domain error OR anything foreign, boxed with a breadcrumb.
/// `data` holds the original thrown value untouched; `message` accumulates context
/// as it bubbles up.
class UnknownError {
    data: unknown,
    message: string[],
}

/// Request/response failures. Concrete provider errors (rate limit, refusal, …)
/// implement this so callers triage without the concrete class.
interface CallError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
}

/// Incremental (streaming) failures. The same concrete error can arise on the call
/// path and mid-stream, so a provider error may implement both `CallError` and this.
interface StreamError {
  function is_network_error(self) -> bool
}

/// Failures on the multi-turn tool-calling path.
interface ToolError {
  function is_network_error(self) -> bool
}
