// User-defined session errors — the typed catalog from the BEP's
// errors-and-retries guide, expressed as ordinary classes.
// `throw X { ... }` + typed `catch` arms + `throws X` signatures all work.

class StepBudgetExceeded {
    message: string,
    steps: int,
}

class CostBudgetExceeded {
    message: string,
    spent_usd: float,
}

// Thrown by the async runner when a run() blocked on cancelled work is
// torn down (`baml.session.Interrupted` in the BEP's error catalog). The
// synchronous reference never throws it: an interrupt is recorded as an
// Interrupted event and the default policy lets the model react
// (04_steering.md); CancelAll only fires cancel tokens.
class SessionInterrupted {
    reason: string,
}
