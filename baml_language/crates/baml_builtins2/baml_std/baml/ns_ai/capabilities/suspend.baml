// Suspend / resume & human-in-the-loop (scenario 44): a run can pause at a labeled
// point, hand back a payload, and be re-entered later from a snapshot. Host persistence
// pending; the sum-typed outcome (`T | Suspend`) is the design-true shape (D5).

/// Opaque, persistable state for re-entering a suspended run.
class Snapshot {
    _data: string,
}

/// The suspended-at-a-point outcome: what to show, and where to resume.
class Suspend {
    payload: string,
    point: string,
}

//baml:llm_capability
/// Runs that can pause and be re-entered. Outcomes are explicit sums — a suspension
/// is a value, not an error.
interface Suspendable requires root.ai.Provider {
    function start<T>(self, prompt: string) -> T | Suspend throws root.errors.UnknownError
    function reenter<T>(self, snap: Snapshot, resume: string) -> T | Suspend throws root.errors.UnknownError
}
