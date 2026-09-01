//! Interface-machinery bodies are anonymous-but-slotted: impl-block methods
//! (in-class or free) and interface default-method bodies are pooled and
//! slotted like any function — direct calls stay `Call(GlobalIndex)` — but
//! they are not addressable items AT ALL: no name map of the compiled
//! `Program` carries them (their `Function::name` is display-only), and
//! every runtime name surface (the runtime name maps, entry-point
//! resolution, suffix matching) skips them.

use baml_tests::{
    baml_test,
    engine::{OptLevel, compile_source_with_opt, try_call_by_name},
};
use bex_engine::BexExternalValue;
use bex_vm_types::Object;

const SOURCE: &str = r#"
interface Greeter {
    function greet(self) -> string throws never {
        "default"
    }
}

class Adopts {
    implements Greeter {}
}

class Overrides {
    implements Greeter {
        function greet(self) -> string throws never { "override" }
    }
}

implement Greeter for int {
    function greet(self) -> string throws never { "int" }
}

function call_virtual(g: Greeter) -> string throws never {
    g.greet()
}

function main() -> string throws never {
    let a: Greeter = Adopts {};
    let b = Overrides {};
    let n: int = 5;
    `${call_virtual(a)}|${b.greet()}|${n.greet()}`
}
"#;

/// The three body shapes this fixture produces, by their display spellings:
/// the interface default body, the in-class override, and the free-impl
/// override. An impl block is anonymous, so overrides spell the synthesized
/// `<(target as iface)>` segment (the lambda convention) — canonical, fully
/// qualified, and identical for the in-class and out-of-body spellings
/// (block syntax is not identity).
const BODY_SPELLINGS: [&str; 3] = [
    "user.Greeter.greet",
    "user.<(user.Overrides as user.Greeter)>.greet",
    "user.<(int as user.Greeter)>.greet",
];

#[test]
fn interface_bodies_are_pooled_and_slotted_but_in_no_name_map() {
    let program = compile_source_with_opt(SOURCE, OptLevel::One);

    for spelling in BODY_SPELLINGS {
        // The body exists: a pooled, flagged function (found by pool
        // enumeration — its display name keys nothing) whose global slot
        // holds it.
        let (object_index, function) = program
            .objects
            .iter()
            .enumerate()
            .find_map(|(idx, obj)| match obj {
                Object::Function(f) if f.name == spelling => Some((idx, f)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("`{spelling}` has no pooled function object"));
        assert!(
            function.is_interface_body,
            "`{spelling}` must carry the body flag"
        );
        assert!(
            program.globals.contains(&bex_vm_types::ConstValue::Object(
                bex_vm_types::ObjectIndex::from_raw(object_index)
            )),
            "`{spelling}`'s function object must own a global slot"
        );
        // And it is absent from every name map of the program.
        assert!(
            !program.function_indices.contains_key(spelling),
            "`{spelling}` leaked into function_indices"
        );
        assert!(
            !program.function_global_indices.contains_key(spelling),
            "`{spelling}` leaked into function_global_indices"
        );
    }

    // Real logical items stay where they were.
    assert!(program.function_indices.contains_key("user.main"));
    assert!(program.function_global_indices.contains_key("user.main"));
}

#[tokio::test]
async fn bodies_dispatch_through_every_road_but_resolve_by_no_name() {
    // All three call forms still reach their bodies: virtual dispatch to the
    // adopted default, the in-class provided method, and the free-impl one.
    let output = baml_test!(SOURCE);
    match output.result.expect("main should run") {
        BexExternalValue::String(s) => {
            assert_eq!(&*s, "default|override|int");
        }
        other => panic!("expected String, got {other:?}"),
    }

    // No body spelling is engine-callable — not exactly, and not through the
    // `user.`-prefix or unambiguous-suffix fallbacks either.
    for name in BODY_SPELLINGS {
        let program = compile_source_with_opt(SOURCE, OptLevel::One);
        let result = try_call_by_name(program, name).await;
        assert!(
            matches!(
                result,
                Err(bex_engine::EngineError::FunctionNotFound { .. })
            ),
            "`{name}` must not be engine-callable; got {result:?}"
        );
    }
}
