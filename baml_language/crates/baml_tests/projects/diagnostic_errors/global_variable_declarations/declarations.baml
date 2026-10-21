// Global variables: the declaration shapes a file-scope `const` refuses.
//
// A global is ONE named binding, optionally annotated: `const NAME = value;`
// or `const NAME: Type = value;`. The shapes a body-level `let` admits beyond
// that have no meaning at file scope, and each is refused at its declaration.

// ── Clean: the two shapes a global takes ─────────────────────────────────────
const PLAIN = 1;
const ANNOTATED: string = "two";

// ── A destructure binds several names ────────────────────────────────────────
const [FIRST, SECOND] = [1, 2];

// ── `_` binds no name at all ─────────────────────────────────────────────────
const _ = 3;

// ── An alias chain binds two names ───────────────────────────────────────────
const ALIAS: let OTHER = 4;

// ── `const … else` has no body to leave ──────────────────────────────────────
const MAYBE: int = 5 else { 6 };

// ── `baml` would hide the package every `spawn` and `env.X` desugars to ─────
const baml = 7;

// ── A global shares the value namespace with functions, clients, and types ──
const Duplicated = 8;

function Duplicated() -> int {
    9
}

// ── The annotation is a declaration-level type ───────────────────────────────
const UNRESOLVED: NoSuchType = 10;
const HOLE: map<string, _> = { "eleven": 11 };
const NOTHING: void = null;

// ── The initializer is checked against the annotation ────────────────────────
const MISMATCHED: string = 12;

// ── A global is declared with `const`; a top-level `let` is refused ──────────
// It still declares the global, so its readers resolve: one error, no cascade.
let WITH_LET = 13;

function reads_with_let() -> int {
    WITH_LET + 1
}
