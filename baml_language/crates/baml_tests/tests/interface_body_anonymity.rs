//! Interface-machinery bodies are anonymous-but-slotted: impl-block methods
//! (in-class or free) and interface default-method bodies are pooled and
//! slotted like any function — direct calls stay `Call(GlobalIndex)` — but
//! they are not runtime-addressable items. Their spellings live only in the
//! compile/link-boundary `Program::body_indices` table, and every runtime
//! name surface (the runtime name maps, entry-point resolution, suffix
//! matching) skips them.

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
/// override.
const BODY_SPELLINGS: [&str; 3] = [
    "user.Greeter.greet",
    "user.Overrides.Greeter.greet",
    "user.Greeter$for$int.greet",
];

#[test]
#[expect(deprecated, reason = "the oracle pins the boundary table's contents")]
fn bodies_live_in_body_indices_not_the_runtime_name_maps() {
    let program = compile_source_with_opt(SOURCE, OptLevel::One);

    for spelling in BODY_SPELLINGS {
        let slots = program.body_indices.get(spelling).unwrap_or_else(|| {
            panic!(
                "`{spelling}` missing from body_indices; present: {:?}",
                program
                    .body_indices
                    .keys()
                    .filter(|k| k.starts_with("user."))
                    .collect::<Vec<_>>()
            )
        });
        // The body is a pooled, flagged function whose global slot holds it.
        let Some(Object::Function(function)) = program.objects.get(slots.object_index) else {
            panic!("`{spelling}` does not point at a function object");
        };
        assert!(
            function.is_interface_body,
            "`{spelling}` must carry the body flag"
        );
        assert_eq!(
            program.globals.get(slots.global_slot),
            Some(&bex_vm_types::ConstValue::Object(
                bex_vm_types::ObjectIndex::from_raw(slots.object_index)
            )),
            "`{spelling}`'s global slot must hold its own function object"
        );
        // And it is absent from every runtime name map.
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
    assert!(!program.body_indices.contains_key("user.main"));
}

#[tokio::test]
async fn bodies_dispatch_through_every_road_but_resolve_by_no_name() {
    // All three call forms still reach their bodies: virtual dispatch to the
    // inherited default, the in-class override, and the free-impl override.
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
