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

/// The cross-capability decision axis (plan D2/D8): concrete errors implement it so a
/// consumer can triage — and a combinator can decide whether re-driving is safe —
/// WITHOUT knowing the concrete class. Answers are author assertions (D2's honest bar:
/// failures move from silent to typed-and-refused, not to statically proven).
///
/// NOTE: `Failure` is MANDATORY on every capability error channel — each channel
/// interface (`CallError`, `StreamError`, `ToolError`, `RealtimeError`) `requires Failure`,
/// so a concrete error that implements any channel must also answer the decision axis and
/// the axis can never be dropped. (This is now expressible: the E0125 cross-package
/// `requires`-satisfaction bug — a user class implementing both interfaces failed the
/// check — is fixed.) Combinators still consult the axis via a runtime `match`; unknown /
/// foreign errors, which carry no `Failure` impl, keep their legacy behavior.
interface Failure {
  function is_retryable(self) -> bool
  function is_effectful(self) -> bool
  function is_policy_refusal(self) -> bool
  function is_resumable(self) -> bool
  function is_unsupported(self) -> bool
}

/// Request/response failures. Concrete provider errors (rate limit, refusal, …)
/// implement this so callers triage without the concrete class.
interface CallError requires Failure {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
}

/// Incremental (streaming) failures. The same concrete error can arise on the call
/// path and mid-stream, so a provider error may implement both `CallError` and this.
interface StreamError requires Failure {
  function is_network_error(self) -> bool
}

/// Failures on the multi-turn tool-calling path.
interface ToolError requires Failure {
  function is_network_error(self) -> bool
}

/// Failures on a live/duplex (realtime) session.
interface RealtimeError requires Failure {
  function is_network_error(self) -> bool
}

// The capability-negotiation error `Unsupported` (thrown by a companion's `_` arm when
// the client's provider lacks the requested capability) must be legal on *every*
// capability channel — so it implements all of them (all classifiers false).
implements Failure for Unsupported {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { false }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { true }
}

implements CallError for Unsupported {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}

implements StreamError for Unsupported {
  function is_network_error(self) -> bool { false }
}

implements ToolError for Unsupported {
  function is_network_error(self) -> bool { false }
}

implements RealtimeError for Unsupported {
  function is_network_error(self) -> bool { false }
}

/// Thrown by `Retry` (and future combinators) instead of silently re-driving an
/// EFFECTFUL provider — a retried submit double-bills, a re-driven realtime session
/// replays external effects (plan D2: refuse-then-error, never silent).
class CannotRetry {
    message: string,
}

implements Failure for CannotRetry {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { true }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { false }
}

implements CallError for CannotRetry {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}

implements StreamError for CannotRetry {
  function is_network_error(self) -> bool { false }
}

implements ToolError for CannotRetry {
  function is_network_error(self) -> bool { false }
}

/// Thrown when a `root.ai.ToolLoop` stop policy halts the loop before the model
/// produced a final value on the plain call path (whose return type has no partial
/// arm). Use `ToolLoop.run_to_budget` to receive the `Budget<T>` outcome instead.
/// A deliberate stop: not retryable (re-driving replays tool side effects).
class LoopBudgetExceeded {
    steps_taken: int,
}

implements Failure for LoopBudgetExceeded {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { true }
  function is_policy_refusal(self) -> bool { true }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { false }
}

implements CallError for LoopBudgetExceeded {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}

implements ToolError for LoopBudgetExceeded {
  function is_network_error(self) -> bool { false }
}
