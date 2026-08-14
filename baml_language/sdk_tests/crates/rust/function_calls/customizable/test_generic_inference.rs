//! Generic *function-call* coverage — the INFERENCE variant (ns_generic_tests).
//!
//! ADAPTATION(rust): intentionally empty — the Rust SDK does no inference.
//!
//! Python's suite pins the engine's inbound-inference phase: bare calls with
//! no subscript / `_types=`, where the engine solves each TypeVar from the
//! argument *values*. The Rust SDK never exercises that phase. rustc solves
//! every type parameter at COMPILE time (a "bare" call is ordinary Rust type
//! inference — where rustc lacks evidence the call does not compile, E0282),
//! and the generated bindings always send the full, explicit TypeVar set on
//! the wire (`CallFunctionArgs.type_args`), so the engine is never asked to
//! infer.
//!
//! Consequently every python case is either redundant or unreachable here:
//! the positive cases are `test_generic_calls.rs` modulo the turbofish, and
//! the negative cases (uninferable, conflicting, or low-evidence bindings
//! rejected engine-side) are compile errors through the typed wrappers. The
//! file is kept on disk, empty, for the cross-language suite checker.
