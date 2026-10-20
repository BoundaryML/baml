// Global variables: what an initializer may not do.
//
// A global's initializer runs once, in the package's `$init`, before any
// request — on a path with no scheduler, no caller to throw to, and an order
// fixed by what each initializer needs. Each rule below is checked where the
// initializer is written, instead of failing engine creation at startup.

// ── Clean: dependency order, not source order ────────────────────────────────
let USES_LATER = LATER + 1;
let LATER = 41;

// ── An initialization cycle, directly ────────────────────────────────────────
let PING = PONG + 1;
let PONG = PING + 1;

// ── A cycle through a function's body ────────────────────────────────────────
let THROUGH_HELPER: int = reads_through_helper();

function reads_through_helper() -> int {
    THROUGH_HELPER + 1
}

// ── A cycle through a parameter default, which every call evaluates ─────────
let THROUGH_DEFAULT: int = with_default();

function with_default(value: int = THROUGH_DEFAULT) -> int {
    value
}

// ── A cycle through a lambda: references are lexical ────────────────────────
// Whether `$init` ever calls the stored function is a question about values;
// initialization order is decided from the source alone, as in Go.
let STORED: () -> int throws never = () -> int { STORED_PEER() };
let STORED_PEER: () -> int throws never = () -> int { STORED() };

// ── A global reading itself ──────────────────────────────────────────────────
let SELF_REFERENCE: int = SELF_REFERENCE + 1;

// ── `return` belongs to function bodies ──────────────────────────────────────
let EARLY_EXIT = {
    if (LATER > 0) {
        return 1;
    }
    2
};

// ── …but a lambda the initializer builds may return ─────────────────────────
let LAMBDA_RETURNS = (x: int) -> int {
    if (x > 0) {
        return x;
    }
    0
};

// ── `spawn` has no scheduler to launch onto during `$init` ──────────────────
let SPAWNED = spawn { 1 };

// ── `await` has nothing to wait on, directly or through a helper ────────────
function waits() -> int {
    let task = spawn { 2 };
    await task
}

let AWAITED = waits();

// ── io is unavailable during `$init` ─────────────────────────────────────────
let HOME = baml.env.get("HOME");

// ── A throw nothing catches would stop the program from starting (warning) ──
function may_fail() -> int throws string {
    throw "failed"
}

let UNCAUGHT = may_fail();

// ── Clean: a caught throw ────────────────────────────────────────────────────
let CAUGHT = may_fail() catch (e) {
    string => 0
};
