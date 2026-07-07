// Provider-held conversation state (scenarios 17, 18, 19): sessions, window compaction,
// and forking. Part of the stateful family — cross-call state with a lifecycle. The
// persistence/host surface is not built, so the handles + interfaces + negotiation are
// real and compiling, with host bodies pending. Methods use the `UnknownError` channel
// for now (per-capability error interfaces are a follow-up; see _plan/deviations.md).

/// Handle to a provider-held conversation (scenario 17).
class Session {
    _id: string,
}

/// Within-run history: the provider threads the conversation for you.
interface Conversational requires root.ai.Provider {
    function chat<T>(self, prompt: string, session: Session) -> T throws root.errors.UnknownError
}

/// An opaque context window, round-tripped verbatim (scenario 18).
class Window {
    _data: string,
}

/// Context-window compaction over a session.
interface Compaction requires root.ai.Provider {
    function window_of(self, session: Session) -> Window throws root.errors.UnknownError
    function compact(self, window: Window) -> Window throws root.errors.UnknownError
}

/// Fork/branch a conversation (scenario 19).
interface Branching requires root.ai.Provider {
    function fork(self, session: Session) -> Session throws root.errors.UnknownError
}
