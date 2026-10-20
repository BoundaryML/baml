// A child namespace's globals, read from the parent as `settings.NAME` and
// reading the parent's as `root.global_variables.NAME`.

let TIMEOUT_SECONDS = 30;
let RETRY_BUDGET = TIMEOUT_SECONDS * root.global_variables.MAX_RETRIES;

class State {
    n: int,
}

// Reached from the parent as `settings.SCALE(..)` and `settings.STATE.n`.
let SCALE = (x: int) -> int {
    x * 2
};
let STATE = State { n: 0 };
let MAYBE_STATE: State? = State { n: 7 };
