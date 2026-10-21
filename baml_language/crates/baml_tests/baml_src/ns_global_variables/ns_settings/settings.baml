// A child namespace's globals, read from the parent as
// `root.global_variables.settings.NAME`, and reading the parent's as
// `root.global_variables.NAME`.

const TIMEOUT_SECONDS = 30;
const RETRY_BUDGET = TIMEOUT_SECONDS * root.global_variables.MAX_RETRIES;

class State {
    n: int,
}

// Reached from the parent as `root.global_variables.settings.SCALE(..)` and
// `root.global_variables.settings.STATE.n`.
const SCALE = (x: int) -> int {
    x * 2
};
const STATE = State { n: 0 };
const MAYBE_STATE: State? = State { n: 7 };

// Spelled like the body parameter a tagged template gives `${name}` in the
// parent namespace's `tagged_template_reads_qualified_global`.
const name = "settings name";
