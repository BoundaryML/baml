//! The linker must FAIL CLOSED on a unit whose data is internally
//! inconsistent, never patch around it: a cached or decoded unit is only as
//! trustworthy as its bytes, and every check in `link` that can reject one
//! must reject it as `LinkError::InvalidUnit` rather than let the VM discover
//! the problem at dispatch.

mod common;

use baml_compiler2_emit::{OptLevel, emit_units};
use bex_vm_types::{
    Object,
    link::{LinkError, link},
    unit::LocalRef,
};
use common::build_db;

const ROOT: &str = "/link-fail-closed";

const SRC: &str = "interface Greeter {\n  function greet(self) -> string throws never\n}\n\
class Dog {\n  implements Greeter {\n    \
function greet(self) -> string throws never {\n      \"woof\"\n    }\n  }\n}\n\
function named_fn() -> int throws never {\n  1\n}\n";

/// An impl rule's provided-method `code_offset` must land on an interface
/// body. A stale or corrupt unit pointing it at a NAMED function would
/// otherwise dispatch `greet` to `named_fn` — the wrong arity, the wrong
/// frame convention — so the link rejects the unit instead.
#[test]
fn rule_code_offset_targeting_a_named_function_is_an_invalid_unit() {
    let db = build_db(ROOT, &[("main.baml", SRC)]);
    let package = db
        .workspace_root()
        .unwrap_or_else(|| unreachable!("the fixture builder adds one workspace root"));
    let mut units = emit_units(&db, package, OptLevel::Two).expect("emit_units succeeds");
    // Positive control: the units link as emitted.
    link(&units).expect("untouched units link");

    let unit = units
        .iter_mut()
        .find(|u| u.source_file == "main.baml")
        .expect("main.baml has a unit");
    // The named function's offset in this unit's `code` bucket.
    let named_offset = unit
        .exports
        .objects
        .iter()
        .find_map(|(name, local)| match local {
            LocalRef::Code(k) if name == "user.named_fn" => Some(*k),
            _ => None,
        })
        .expect("named_fn is exported from the code bucket");
    assert!(
        matches!(&unit.code[named_offset as usize], Object::Function(f) if !f.is_interface_body),
        "control: the target must be a named (non-body) function"
    );
    // Retarget the impl's provided `greet` at it.
    let (_, rules) = unit
        .package_fragment
        .impl_rules
        .iter_mut()
        .find(|(iface, _)| iface == "user.Greeter")
        .expect("the Greeter impl rule rides main.baml's unit");
    let (_, method) = rules[0]
        .methods
        .iter_mut()
        .find(|(name, _)| name == "greet")
        .expect("the rule provides `greet`");
    method.code_offset = named_offset;

    match link(&units) {
        Err(LinkError::InvalidUnit(message)) => assert!(
            message.contains("not an interface body"),
            "the rejection must name the body invariant; got: {message}"
        ),
        other => panic!("a rule aimed at a named function must be an invalid unit; got {other:?}"),
    }
}
