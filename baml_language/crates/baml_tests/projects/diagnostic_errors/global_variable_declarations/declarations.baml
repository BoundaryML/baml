// Global variables: the declaration shapes a file-scope `let` refuses.
//
// A global is ONE named binding, optionally annotated: `let NAME = value;` or
// `let NAME: Type = value;`. The shapes a body-level `let` admits beyond that
// have no meaning at file scope, and each is refused at its declaration.

// ── Clean: the two shapes a global takes ─────────────────────────────────────
let PLAIN = 1;
let ANNOTATED: string = "two";

// ── A destructure binds several names ────────────────────────────────────────
let [FIRST, SECOND] = [1, 2];

// ── `_` binds no name at all ─────────────────────────────────────────────────
let _ = 3;

// ── An alias chain binds two names ───────────────────────────────────────────
let ALIAS: let OTHER = 4;

// ── `let … else` has no body to leave ────────────────────────────────────────
let MAYBE: int = 5 else { 6 };

// ── `baml` would hide the package every `spawn` and `env.X` desugars to ─────
let baml = 7;

// ── A global shares the value namespace with functions, clients, and types ──
let Duplicated = 8;

function Duplicated() -> int {
    9
}

// ── The annotation is a declaration-level type ───────────────────────────────
let UNRESOLVED: NoSuchType = 10;
let HOLE: map<string, _> = { "eleven": 11 };
let NOTHING: void = null;

// ── The initializer is checked against the annotation ────────────────────────
let MISMATCHED: string = 12;

// ── `const` is accepted, and is the same as `let` for a global ───────────────
const CONSTANT = 13;
