class Done<T> {
    value: T,
    metadata: ResponseMetadata,
    conversation: Conversation,

    /// Projects this completed turn into the direct-call response shape —
    /// the same value/metadata/conversation triple a generated function's
    /// lowering produces.
    function response(self) -> ResponseWithMetadata<T> throws never {
        ResponseWithMetadata<T> {
            value: self.value,
            metadata: self.metadata,
            conversation: self.conversation,
        }
    }

    // Whole-run accounting: cumulative across every model step of this run,
    // duplicated here for uniform access alongside the stop variants
    // (`metadata.usage` carries the same cumulative value on Done).
    steps_taken: int,
    usage: Usage,
}

// A voluntary policy stop: the runner's `max_steps` limit, its `stop_when`
// predicate, or a `StepPlan` stop decided the run should not take another
// model step. Cumulative usage is carried so a resuming caller can account
// for what this run already spent.
class Stopped {
    conversation: Conversation,
    steps_taken: int,
    usage: Usage,
    reason: string, // "max_steps", "stop_when", or a StepPlan stop's reason
}

class Handoff {
    // The exact provider call is retained so the application can correlate a
    // ToolResult and submit it before resuming this conversation.
    call: ai.tools.ToolCall,
    conversation: Conversation,
    steps_taken: int,
    usage: Usage,
}

// Cooperative cancellation returns the last fully committed provider
// conversation. No application tool batch is left partially submitted, so the
// caller may resume this checkpoint with a fresh Agent run.
class Interrupted {
    conversation: Conversation,
    steps_taken: int,
    usage: Usage,
    reason: string,
}

/// An involuntary stop: a classified failure occurred AFTER this run had
/// committed progress — a continuation's entry append or submit, or at
/// least one completed provider step. The failure rides INSIDE the value
/// because the value is what you need to continue: the conversation is the
/// last committed state, and `session.resume()` (or `submit_tool_results`,
/// if the stop left pending calls) picks it up without re-appending
/// messages or re-running tools.
///
/// A failure BEFORE any progress still throws — nothing was committed, so
/// the ordinary catch patterns (back off on `RateLimited`, rethrow
/// `Refused`) and step-level `ai.retry` semantics are unchanged. The
/// invariant: an Agent run either returns an outcome at a committed state,
/// or throws having changed nothing.
class Failed {
    cause: Failure | baml.errors.UnknownError,
    conversation: Conversation,
    steps_taken: int,
    usage: Usage,
}

/// Thrown by `task.complete()` and `session.complete(...)` when a run that
/// was expected to produce its final typed value stopped at a resumable
/// boundary instead.
///
/// This is a channel conversion, not a fifth outcome. The same three stop
/// states that `Agent.run` returns as values — `Stopped`, `Handoff`,
/// `Interrupted` — become an error only at call sites that declared
/// incompleteness exceptional. The conversion is lossless: the carried
/// outcome still holds the committed conversation, so a catch site can
/// inspect the cause and resume. `Done` is unrepresentable here by
/// construction; the field type is the proof that unwrapping cannot misfire.
class IncompleteRun {
    outcome: Stopped | Handoff | Interrupted,

    /// The committed conversation at the stop, whichever state it was.
    /// A live reference, not a snapshot: act on the error before continuing
    /// the session it came from, or fork() first for forensics.
    function conversation(self) -> Conversation throws never {
        match (self.outcome) {
            let stopped: Stopped => stopped.conversation,
            let handoff: Handoff => handoff.conversation,
            let interrupted: Interrupted => interrupted.conversation,
        }
    }

    /// Steps completed before the stop.
    function steps_taken(self) -> int throws never {
        match (self.outcome) {
            let stopped: Stopped => stopped.steps_taken,
            let handoff: Handoff => handoff.steps_taken,
            let interrupted: Interrupted => interrupted.steps_taken,
        }
    }
}

// IncompleteRun deliberately does NOT implement `ai.Failure`. A demanded
// completion that stopped at a resumable boundary is control flow, not a
// fault — a generic `ai.Failure` catch arm must never silently absorb it.
// It is its own term in every throws union that can carry it, so catch
// sites are forced to decide what a stop means for them.
