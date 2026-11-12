//! A package compiled at run time (`reflect.Package.compile`, a session
//! submission) pools its string literals like the startup pool does: each is
//! a string cell every reader understands, not an `Object::String` enum cell
//! left over from the compiler's pool. Pinned on the two readers that caught
//! the difference: a method call on the literal (`as_string`) and a map
//! lookup keyed by it (structural hashing).

use std::sync::Arc;

use baml_db::OptLevel;
use bex_engine::{BexEngine, BexExternalValue as Ext, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
const RUNTIME_SOURCE: string = `
    function greet() -> string { "hello from the pool" }
    function keyed() -> int {
        let m: map<string, int> = { "k": 7 };
        m.get("k") ?? 0
    }
`;

function RuntimeLiteralLength() -> int {
    let package = reflect.Package.compile({ "main.baml": RUNTIME_SOURCE });
    let greet = package.get_function<() -> string>("root.greet");
    greet().length()
}

function RuntimeLiteralEquals() -> bool {
    let package = reflect.Package.compile({ "main.baml": RUNTIME_SOURCE });
    let greet = package.get_function<() -> string>("root.greet");
    greet() == "hello from the pool"
}

function RuntimeMapLiteralKey() -> int {
    let package = reflect.Package.compile({ "main.baml": RUNTIME_SOURCE });
    let keyed = package.get_function<() -> int>("root.keyed");
    keyed()
}
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_runtime_compiler(
            baml_tests::engine::compile_source_with_opt(SOURCE, OptLevel::One),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("the engine starts"),
    )
}

async fn call(engine: &Arc<BexEngine>, name: &str) -> Ext {
    engine
        .call_function(
            name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap_or_else(|error| panic!("`{name}` fails: {error}"))
}

#[tokio::test]
async fn a_runtime_packages_literal_is_a_string_cell() {
    let engine = engine();
    assert_eq!(
        call(&engine, "RuntimeLiteralLength").await,
        Ext::Int("hello from the pool".len() as i64)
    );
    assert_eq!(call(&engine, "RuntimeLiteralEquals").await, Ext::Bool(true));
}

#[tokio::test]
async fn a_runtime_packages_literal_hashes_as_a_map_key() {
    let engine = engine();
    assert_eq!(call(&engine, "RuntimeMapLiteralKey").await, Ext::Int(7));
}
