//! Tests for the BamlError / BamlPanic delivery contract.
//!
//! Pins the target behavior before it is implemented: a thrown BAML value
//! must surface in Rust as a `baml_bridge::Error::Thrown` *wrapper* carrying the
//! decoded value via `value` — a plain struct, codegen'd by the normal rules
//! — rather than as a stringified catch-all. User panics surface as
//! `baml_bridge::Error::Panic`, and infra/undeclared errors as
//! `baml_bridge::Error::Runtime`. This module stays gated off until rich error
//! decoding lands; once it compiles, each case below asserts the per-arm
//! contract.
//!
//! Exit (`baml.sys.exit`) can't be observed by matching an `Err` because the
//! decode path terminates the process, which is uncatchable. The intent is
//! to assert the *process* exit code by running a standalone snippet in a
//! subprocess (a child exit can't take the test runner down) and asserting
//! its return code — covering both a non-zero code and `exit(0)`, the case
//! the `is_exit_panic` discriminator exists to protect. See the divergence
//! note on `test_clean_exit_terminates_process_with_code`.

use std::time::Duration;

use baml_bridge::Map;
// SPECULATIVE: the low-level bridge surface mirrors python's `baml_bridge`
// (`get_runtime`, `call_function`, `BamlCallContext`); provisional until the
// Rust bridge pins it.
use baml_bridge::runtime::{BamlCallContext, call_function, get_runtime};
use baml_sdk::baml::json::JsonParseError;
// SPECULATIVE: `LoadDocError`, the generated error type for the union
// `throws ParseError | TimeoutError` contract, is a provisional name — the
// generator's naming for throws-union enums is not pinned yet.
use baml_sdk::raises_test::{LoadDoc, LoadDocError, ParseError, Reparse};
use baml_sdk::throws_test::{DoPanic, MyError, ParseJson, ThrowMyError};

// stdlib native builtins (`baml.json.parse`, `baml.sys.*`) can't be called as
// top-level entry points, so the fixture wraps each in a bytecode function
// (ParseJson / DoPanic / DoExit / SleepMs) that the host calls.

const _BAD_JSON: &str = "{not valid json";

// `baml.json.parse` on bad input → `Error::Thrown` whose `value` is a
// `JsonParseError` (a plain generated struct). Proves stdlib error classes
// surface structured, independent of any `throws` clause.
#[test]
fn test_errors_stdlib_error_surfaces_as_baml_error() {
    match ParseJson(_BAD_JSON.to_string()) {
        Err(baml_bridge::Error::Thrown { value, .. }) => {
            // `isinstance(..., JsonParseError)` is the static type of `value`.
            let _: JsonParseError = value;
        }
        Err(other) => panic!("expected Error::Thrown, got {other:?}"),
        Ok(_) => panic!("expected Error::Thrown, got Ok"),
    }
}

// A user `throw` of a declared error → `Error::Thrown` whose `value` is
// the declared user error instance itself (not a `.code` sub-field).
#[test]
fn test_errors_user_throw_surfaces_declared_instance() {
    match ThrowMyError() {
        Err(baml_bridge::Error::Thrown { value, .. }) => {
            let _: MyError = value;
        }
        Err(other) => panic!("expected Error::Thrown, got {other:?}"),
        Ok(_) => panic!("expected Error::Thrown, got Ok"),
    }
}

// A throw escaping a *multi-member* `throws` union must carry the thrown
// value's class FQN in `class_name`, exactly like a single-member throws.
//
// Regression for the bridge-dogfood bug: the engine wraps a thrown value in
// `union_variant_value` for a multi-member `throws`, and the FQN reader only
// unwrapped a top-level `class_value` — so `class_name` came back `None` for
// union throws while the value still decoded fine. `Reparse` declares
// `throws ParseError` (single) and `LoadDoc` declares
// `throws ParseError | TimeoutError` (union); both throw `ParseError`, so
// their `class_name` must agree.
#[test]
fn test_errors_union_throws_preserves_class_name() {
    // DIVERGENCE(rust): `Error::Thrown` has no runtime `class_name` field —
    // the thrown class (`user.raises_test.ParseError`) is carried by the
    // static type instead. The single-throws value decodes to `ParseError`
    // itself; the union-throws value decodes to the generated throws-union
    // enum, whose variant is the class identity. Both arms naming
    // `ParseError` is the `class_name` agreement.
    match Reparse("x".to_string()) {
        Err(baml_bridge::Error::Thrown { value, .. }) => {
            let _: ParseError = value;
        }
        Err(other) => panic!("expected Error::Thrown, got {other:?}"),
        Ok(_) => panic!("expected Error::Thrown, got Ok"),
    }
    match LoadDoc("x".to_string()) {
        // SPECULATIVE: the throws-union enum's variant shape is provisional.
        Err(baml_bridge::Error::Thrown {
            value: LoadDocError::ParseError(value),
            ..
        }) => {
            let _: ParseError = value;
        }
        Err(other) => panic!("expected a thrown ParseError variant, got {other:?}"),
        Ok(_) => panic!("expected Error::Thrown, got Ok"),
    }
}

// A host-side invalid argument (an extra kwarg the function doesn't
// declare) → `BamlError` wrapping `baml.errors.InvalidArgument`,
// synthesized host-side rather than thrown from the VM.
#[test]
fn test_errors_host_invalid_argument_wraps_baml_errors_invalid_argument() {
    // DIVERGENCE(rust): an argument the function doesn't declare cannot be
    // passed through the typed signature — it is a compile error, not a
    // host-synthesized `baml.errors.InvalidArgument`. Compile-time coverage
    // belongs to `optional_args_static.rs`-style compile-fail probes.
}

// A user-initiated panic via `baml.sys.panic` → `Error::Panic` (python:
// `BamlPanic` whose `.value` is a `baml.panics.UserPanic`, routed by the
// namespace check, distinct from a host-synthesized `SdkPanic`).
#[test]
fn test_errors_user_panic_surfaces_as_baml_panic() {
    // DIVERGENCE(rust): `Error::Panic` carries only the rendered message +
    // trace — the `UserPanic`-vs-`SdkPanic` class routing is not decodable
    // host-side, so the arm match is the assertion.
    let result = DoPanic("user-initiated boom".to_string());
    assert!(matches!(result, Err(baml_bridge::Error::Panic { .. })));
}

// Async cancellation: python maps it to `asyncio.CancelledError` with a BAML
// reason on the awaiting task.
#[tokio::test]
async fn test_errors_cancellation_surfaces_as_baml_panic() {
    // DIVERGENCE(rust): tokio has no cross-task cancellation exception — the
    // aborted call itself resolves to `Err(Error::Panic)` (the cancellation
    // panic) instead of a `CancelledError` carrying a `reason`.
    let rt = get_runtime();
    let ctx = BamlCallContext::new();

    let _abort_soon = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        ctx.abort();
    };

    let (result, ()) = tokio::join!(
        call_function(
            rt,
            "user.throws_test.SleepMs",
            Map::from([("ms".to_string(), 2000)]),
            &ctx,
        ),
        _abort_soon
    );
    assert!(matches!(result, Err(baml_bridge::Error::Panic { .. })));
}

// `str(e)` is non-empty — guards the `@trace` / telemetry path, which
// records `str(e)`.
#[test]
fn test_errors_str_is_non_empty() {
    let err = match ParseJson(_BAD_JSON.to_string()) {
        Err(err) => err,
        Ok(_) => panic!("expected ParseJson to fail"),
    };
    // python `str(e)` → the `Display` rendering.
    assert!(!err.to_string().is_empty());
}

// ---------------------------------------------------------------------------
// BAML traceback. The thrown error carries the BAML stack as a list of
// pre-rendered `File "<src>", line N, in <fn>` strings in `trace`; python
// additionally splices them into the exception's real Python traceback so
// `traceback.format_exception` renders the `.baml` source frame as an
// ordinary traceback line.
// ---------------------------------------------------------------------------

/// `File "<src>", line N, in <fn>` — the wire trace-line shape. (The python
/// suite pins this as a regex; [`_parse_trace_line`] is the structural
/// equivalent here.)
const _TRACE_LINE: &str = r#"File "(?P<file>[^"]*)", line (?P<line>\d+), in (?P<func>[^"]+)"#;

/// Parses a wire trace line into `(file, line, func)`, or `None` if the line
/// is not in the [`_TRACE_LINE`] shape.
fn _parse_trace_line(line: &str) -> Option<(&str, u64, &str)> {
    let rest = line.strip_prefix("File \"")?;
    let (file, rest) = rest.split_once("\", line ")?;
    let (line_no, func) = rest.split_once(", in ")?;
    if func.is_empty() {
        return None;
    }
    Some((file, line_no.parse().ok()?, func))
}

/// Python tracebacks are 1-indexed, even for synthetic builtin frames.
/// PROVISIONAL: whether the Rust error rendering applies the same clamp to
/// line-0 builtin frames is unpinned; this mirrors the python normalization
/// until the bridge decides.
fn _python_traceback_line(line: &str) -> String {
    let Some((file, line_no, func)) = _parse_trace_line(line) else {
        return line.to_string();
    };
    let line_no = line_no.max(1);
    format!(r#"File "{file}", line {line_no}, in {func}"#)
}

// `trace` is the list of rendered BAML stack frames (one per frame),
// pointing into the throwing function's `.baml` source (python surfaces it
// as `.baml_trace`).
#[test]
fn test_errors_baml_error_carries_baml_trace() {
    let trace = match ThrowMyError() {
        Err(baml_bridge::Error::Thrown { trace, .. }) => trace,
        Err(other) => panic!("expected Error::Thrown, got {other:?}"),
        Ok(_) => panic!("expected Error::Thrown, got Ok"),
    };
    assert!(
        !trace.is_empty(),
        "expected a non-empty list, got {trace:?}"
    );
    // Most-recent-call-last: the throwing function is the last frame.
    let last = trace.last().unwrap();
    let Some((file, line, func)) = _parse_trace_line(last) else {
        panic!("trace line not in `File ..., line N, in fn` form: {last:?}");
    };
    assert!(file.ends_with("types.baml"), "{file}");
    assert_eq!(func, "user.throws_test.ThrowMyError");
    assert!(line >= 1);
}

// The BAML frames are spliced into the exception's Python traceback, so
// `traceback.format_exception` renders the `.baml` source frame inline (not
// as a detached blob).
#[test]
fn test_errors_baml_trace_spliced_into_python_traceback() {
    // DIVERGENCE(rust): there is no host traceback to splice into. The
    // closest surface is the error's `Display` rendering, so this asserts
    // every wire trace line is rendered there inline.
    let err = match ParseJson(_BAD_JSON.to_string()) {
        Err(err) => err,
        Ok(_) => panic!("ParseJson did not raise BamlError"),
    };
    let rendered = err.to_string();
    let wire_trace = match err {
        baml_bridge::Error::Thrown { trace, .. } => trace,
        other => panic!("expected Error::Thrown, got {other:?}"),
    };

    // Every wire trace line must appear in the rendered error. Builtin
    // frames may carry line 0 on the wire, but Python traceback objects
    // render locations as 1-indexed lines.
    for line in &wire_trace {
        let expected = _python_traceback_line(line);
        assert!(
            rendered.contains(&expected),
            "{expected:?} not spliced into:\n{rendered}"
        );
    }
    // ...and the splice must name the throwing BAML function + its source.
    assert!(
        rendered.lines().any(|line| {
            _parse_trace_line(line.trim()).is_some_and(|(file, _, func)| {
                file.ends_with("types.baml") && func.starts_with("user.throws_test.ParseJson")
            })
        }),
        "{rendered}"
    );
}

// ---------------------------------------------------------------------------
// Clean exit — subprocess, outside the test runner's caught-failure
// machinery. `baml.sys.exit(code)` must terminate the process with `code`,
// NOT surface as a catchable `Error::Panic`. Both a non-zero code and
// `exit(0)` are covered: zero is exactly what the `is_exit_panic` bool
// discriminator protects (proto3 can't tell exit-code-0 from "no exit").
// ---------------------------------------------------------------------------

// python parametrizes this over exit codes [0, 7], running
// `DoExit(code); print("UNREACHABLE")` as a standalone interpreter snippet in
// a subprocess and asserting the child's return code (and that "UNREACHABLE"
// never printed).
#[test]
fn test_errors_clean_exit_terminates_process_with_code() {
    // DIVERGENCE(rust): the same observation needs a subprocess harness that
    // re-runs the current test binary (or a helper binary) so the child can
    // call `throws_test::DoExit(code)` and the parent can assert
    // `ExitStatus::code()` for both 0 and 7. No such harness exists in this
    // suite yet, so the port carries the intent without a runtime body.
}
