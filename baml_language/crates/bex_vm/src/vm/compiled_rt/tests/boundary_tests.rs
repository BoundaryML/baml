//! The class check where a value enters compiled code from a dynamic source.
//!
//! Compiled field access trusts a local's class type (`FieldBase::typed`).
//! The emitter checks a value once where it enters such a local from a
//! dynamic source: a boxed entry's argument (from the interpreter or the
//! host), a call the VM makes, a builtin's or a generic callee's result
//! (JSON decode, reflection), a dispatch's VM fallback. Every site emits
//! `vm.entering_class(value, word)`; `baml_tests`' `rust_backend` asserts
//! the placement. Here a compiled body for `Take(v: unknown)` checks its
//! argument against `Point` as a boxed entry does, and the interpreter
//! passes it each kind of value such a source could deliver.

use std::sync::atomic::AtomicU32;

use super::*;
use crate::compiled_rt::{class_word, debug_class};

static POINT: AtomicU32 = AtomicU32::new(0);
static OTHER: AtomicU32 = AtomicU32::new(0);

/// `Take`'s compiled body: the boxed entry's check, then 1 for a `Point`.
fn take(vm: &mut BexVm, _: &FrameRecord, args: &[Value]) -> BodyResult {
    let word = class_word(object(vm, &POINT));
    let point = vm.entering_class(args[0], word)?;
    Ok(Value::int(i64::from(!point.is_null())))
}

fn run(argument: &str) -> String {
    let source = format!(
        r#"
class Point {{ x int }}
class Other {{ name string }}
function Take(v: unknown) -> int {{ 0 }}
function Main() -> string {{ Take({argument}).to_string() }}
"#
    );
    run_with_compiled_bodies(
        &source,
        &[("user.Take", take)],
        &[("user.Point", &POINT), ("user.Other", &OTHER)],
        false,
    )
}

#[test]
fn a_value_of_the_static_class_or_null_enters() {
    assert_eq!(run("Point { x: 1 }"), "1");
    assert_eq!(run("null"), "0");
}

#[test]
fn an_instance_of_another_class_is_refused_naming_both() {
    let out = run(r#"Other { name: "o" }"#);
    assert!(out.starts_with("error:"), "{out}");
    assert!(out.contains("CompiledBody"), "{out}");
    assert!(out.contains("Other") && out.contains("Point"), "{out}");
}

#[test]
fn a_non_instance_is_the_interpreters_type_error() {
    // A string (a builtin's result), a map (a JSON decode of an object into
    // a dynamic type), a list (a builtin's), an int (a host scalar).
    for (argument, got) in [
        (r#""text""#, "String"),
        (r#"baml.json.decode<map<string, int>>("{\"a\": 1}")"#, "Map"),
        (r#""a,b".split(",")"#, "Array"),
        ("7", "Int"),
    ] {
        let out = run(argument);
        assert!(out.contains("TypeError"), "{argument}: {out}");
        assert!(out.contains("Instance"), "{argument}: {out}");
        assert!(out.contains(got), "{argument}: {out}");
    }
}

#[test]
fn the_debug_assertion_passes_the_static_class_and_null() {
    fn check(vm: &mut BexVm, _: &FrameRecord, args: &[Value]) -> BodyResult {
        let word = class_word(object(vm, &POINT));
        Ok(Value::int(i64::from(!debug_class(args[0], word).is_null())))
    }
    let source = r#"
class Point { x int }
class Other { name string }
function Take(v: unknown) -> int { 0 }
function Main() -> string { (Take(Point { x: 1 }) + Take(null)).to_string() }
"#;
    let out = run_with_compiled_bodies(
        source,
        &[("user.Take", check)],
        &[("user.Point", &POINT), ("user.Other", &OTHER)],
        false,
    );
    assert_eq!(out, "1");
}

#[test]
#[should_panic(expected = "whose class is not its static type")]
fn the_debug_assertion_panics_on_another_class() {
    fn check(vm: &mut BexVm, _: &FrameRecord, args: &[Value]) -> BodyResult {
        let word = class_word(object(vm, &POINT));
        Ok(debug_class(args[0], word))
    }
    let source = r#"
class Point { x int }
class Other { name string }
function Take(v: unknown) -> unknown { v }
function Main() -> string { Take(Other { name: "o" }); "" }
"#;
    let _ = run_with_compiled_bodies(
        source,
        &[("user.Take", check)],
        &[("user.Point", &POINT), ("user.Other", &OTHER)],
        false,
    );
}
