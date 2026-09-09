//! Caller shapes survive bridge handles, specialization and native dispatch.
mod common;
use bex_engine::{BexEngine, BexExternalValue as V, FunctionCallContextBuilder};
use std::sync::Arc;
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[tokio::test]
async fn optional_function_layouts_across_source_bridge_and_reflection() {
    let program = common::compile_for_engine(
        r#"
function format(text: string, suffix: string = "?", extra: string = ".", prefix: string = "Hi") -> string throws never {
    prefix + text + suffix + extra
}
function make() -> (text: string, suffix?: string, extra?: string, prefix?: string) -> string throws never { format }
function narrow(f: (string) -> string throws never) -> string throws never { f("Ada") }
function named(f: (value: string, prefix?: string, suffix?: string) -> string throws never) -> string throws never {
    f("Ada", prefix = "<", suffix = ">")
}
function native(f: (string) -> string throws never) -> string[] throws never { ["Ada", "Bea"].map(f) }
function reflected(f: reflect.AnyFunction<Returns = string, Throws = never>) -> string {
    reflect.call_any(f, { "text": "Ada", "prefix": "<", "suffix": ">" })
}
function source_narrow() -> string throws never { narrow(format) }
function source_named() -> string throws never { named(format) }
function named_id(f: (value: string, prefix?: string, suffix?: string) -> string throws never) -> string {
    f("Ada", suffix = ">", prefix = "<", $id = boundary.id())
}
function source_native() -> string[] throws never { native(format) }
function local_named() -> string throws never {
    let f: (value: string, prefix?: string, suffix?: string) -> string throws never = format;
    f("Ada", prefix = "<", suffix = ">")
}
function generic<T>(value: T, extra: string = ".") -> T throws never { value }
function local_generic() -> string {
    let f: (string) -> string throws never = generic<string>;
    f("Ada", $id = boundary.id())
}
function make_generic() -> (value: string, extra?: string) -> string throws never { generic<string> }
"#,
    );
    let local = program
        .objects
        .iter()
        .find_map(|object| match object {
            bex_vm_types::Object::Function(function) if function.name == "user.local_named" => {
                Some(function)
            }
            _ => None,
        })
        .expect("compiled local call");
    assert!(
        local
            .bytecode
            .instructions
            .iter()
            .enumerate()
            .any(|(pc, instruction)| {
                matches!(instruction, bex_vm_types::Instruction::Call { .. })
                    && local
                        .bytecode
                        .call_layouts
                        .get(&pc)
                        .is_some_and(|layout| layout.len() == 3)
            }),
        "constant-call optimization must retain the narrower caller layout"
    );
    let engine =
        Arc::new(BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![]).unwrap());
    let handle = engine
        .call_function("make", vec![], context(), true)
        .await
        .unwrap();
    for (entry, args, expected) in [
        ("narrow", vec![handle.clone()], V::String("HiAda?.".into())),
        ("named", vec![handle.clone()], V::String("<Ada>.".into())),
        ("named_id", vec![handle.clone()], V::String("<Ada>.".into())),
        ("local_generic", vec![], V::String("Ada".into())),
        (
            "reflected",
            vec![handle.clone()],
            V::String("<Ada>.".into()),
        ),
        ("source_narrow", vec![], V::String("HiAda?.".into())),
        ("source_named", vec![], V::String("<Ada>.".into())),
        ("local_named", vec![], V::String("<Ada>.".into())),
    ] {
        assert_eq!(
            engine
                .call_function(entry, args, context(), true)
                .await
                .unwrap_or_else(|e| panic!("{entry}: {e:?}")),
            expected,
            "{entry}"
        );
    }
    for (entry, args) in [("native", vec![handle]), ("source_native", vec![])] {
        let result = engine
            .call_function(entry, args, context(), true)
            .await
            .unwrap_or_else(|e| panic!("{entry}: {e:?}"));
        let V::Array { items, .. } = result else {
            panic!("{entry}: expected array")
        };
        assert_eq!(
            items,
            vec![V::String("HiAda?.".into()), V::String("HiBea?.".into())]
        );
    }
    let generic = engine
        .call_function("make_generic", vec![], context(), true)
        .await
        .unwrap();
    assert_eq!(
        engine
            .call_function("narrow", vec![generic], context(), true)
            .await
            .unwrap(),
        V::String("Ada".into())
    );
}
