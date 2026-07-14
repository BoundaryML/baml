//! Compile-fail probes for the optional-argument surface — the static
//! counterpart of `test_optional_args.rs`, ported from the pyright-negative
//! probes in `optional_args_static.py` (each `# pyright: ignore[...]` line
//! there is a call the type checker must reject).
//!
//! Rust's analogue of a pyright-negative probe is a compile-fail test, which
//! needs a trybuild-style harness this suite does not have yet. Until one
//! exists, each probe is preserved below as a commented-out snippet; none of
//! this module is live code.
//!
//! Shared preamble for every snippet:
//!
//! ```text
//! use baml_bridge::OptionalArg::Unset;
//! use baml_sdk::optional_args_probe;
//! ```

// `optional_args_probe()` — pyright: reportCallIssue. No arguments at all:
// `arg0` is required (and Rust has no default arguments, so the optional
// slots are missing too).
// compile_fail: optional_args_probe();

// `optional_args_probe(1, 8)` — pyright: reportCallIssue. python rejects a
// *positional* value for the keyword-only optional; Rust rejects the arity
// (every parameter is always passed).
// compile_fail: optional_args_probe(1, 8);

// `optional_args_probe(1, opt3=1)` — pyright: reportCallIssue. An unknown
// keyword argument; Rust has no keyword arguments, so the analogue is an
// extra argument the signature does not declare.
// compile_fail: optional_args_probe(1, Unset, Unset, 1);

// `optional_args_probe("x")` — pyright: reportArgumentType. A string where
// `arg0: int` is declared.
// compile_fail: optional_args_probe("x".to_string(), Unset, Unset);

// `optional_args_probe(1, opt1="x")` — pyright: reportArgumentType. A string
// where the optional `opt1: int?` slot is declared.
// compile_fail: optional_args_probe(1, Some("x".to_string()), Unset);
