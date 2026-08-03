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

class SessionInterrupted {
    reason: string,
}
