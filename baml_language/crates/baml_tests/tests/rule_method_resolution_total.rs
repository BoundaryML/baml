//! Method resolution off an impl rule is TOTAL: for an accepted program every
//! answer other than a callee is an invariant break, reported as the
//! `VmInternalError` naming its kind — never a "not found" a caller could
//! read as a benign neighbor (no impl, structural fallback, unresolved call).
//! BAML cannot write a rule that lacks a required method (the compiler
//! rejects the impl), so the table is corrupted AFTER compilation — the shape
//! a stale or corrupt cached unit produces.

use baml_tests::engine::{OptLevel, compile_source_with_opt, try_call_by_name};
use bex_engine::{BexExternalValue, EngineError};
use bex_vm::errors::VmInternalError;
use bex_vm_types::types::Program;

const SOURCE: &str = r#"
interface Greeter {
    function greet(self) -> string throws never
}

class Dog {
    implements Greeter {
        function greet(self) -> string throws never { "woof" }
    }
}

function through_interface(g: Greeter) -> string throws never {
    g.greet()
}

function main() -> string throws never {
    through_interface(Dog {})
}
"#;

/// Strip the provided `greet` row from `Dog`'s `Greeter` rule, leaving a rule
/// whose interface requires a method nothing supplies.
fn strip_provided_greet(program: &mut Program) {
    let Program {
        packages, objects, ..
    } = program;
    let rule = packages
        .values_mut()
        .flat_map(|pkg| pkg.impl_rules.values_mut().flatten())
        .find(|rule| {
            objects[rule.interface_head]
                .as_interface()
                .is_some_and(|def| def.name.name().as_str() == "Greeter")
        })
        .expect("the Greeter rule is baked");
    let before = rule.methods.len();
    rule.methods.retain(|name, _| name.as_str() != "greet");
    assert_eq!(
        rule.methods.len(),
        before - 1,
        "control: the rule provided `greet`"
    );
}

#[tokio::test]
async fn a_rule_missing_a_required_method_is_an_internal_error_not_a_miss() {
    // Positive control: the untouched program dispatches to the provided body.
    let program = compile_source_with_opt(SOURCE, OptLevel::One);
    match try_call_by_name(program, "user.main").await {
        Ok(BexExternalValue::String(s)) => assert_eq!(&*s, "woof"),
        other => panic!("control: main runs through the provided method; got {other:?}"),
    }

    let mut program = compile_source_with_opt(SOURCE, OptLevel::One);
    strip_provided_greet(&mut program);
    let err = match try_call_by_name(program, "user.main").await {
        Err(err) => err,
        Ok(value) => {
            panic!("a rule missing its required method must not dispatch; got {value:?}")
        }
    };
    let internal = match &err {
        EngineError::VmInternalError(source)
        | EngineError::TracedVmInternalError { source, .. } => source,
        other => panic!("expected a VM internal error, got {other:?}"),
    };
    assert!(
        matches!(
            internal,
            VmInternalError::UnprovidedRequiredMethod { interface, method }
                if interface == "user.Greeter" && method == "greet"
        ),
        "the error must name the unprovided required method, not read the miss as \
         an unresolved call; got {internal:?}"
    );
}
