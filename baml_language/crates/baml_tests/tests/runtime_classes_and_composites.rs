//! BEP-066 Scenarios 2 and 3: runtime classes and composites flow
//! through offline LLM companions, keep pointing at their declarations, and
//! remain usable through the dynamic access/JSON surfaces.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use bex_external_types::{BexExternalAdt, TaggedHeapHandleKind};

/// A runtime-created class has no spelling a host can resolve, and its bare
/// item name collides with a *compiled* class of the same name — an echoed
/// value would rebind to that declaration and violate its contract. So its
/// instance crosses as an opaque handle rather than structurally.
#[tokio::test]
async fn an_anonymous_class_instance_crosses_as_an_opaque_handle() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        /// The compiled declaration whose item name the runtime one repeats.
        class Widget {
            name: string,
        }

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> unknown {
            let widget_t = reflect.class.new("Widget", {
                "name": reflect.Type.of<string>(),
            })
            type Widget = unreflect(widget_t.as_type())
            Extract@parse<Widget>(`{"name":"anonymous"}`)
        }
        "##
    );

    match output
        .result
        .expect("a runtime class instance should reach the host")
    {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::RuntimeValue,
            ..
        }) => {}
        other => panic!(
            "an anonymous class instance must cross as a tagged RuntimeValue, \
             not structurally under the compiled `Widget`: {other:?}"
        ),
    }
}

/// The same for a *runtime-compiled package* member, which is the case a
/// name-based guard misses: it is a `Declared` name like any static class, and
/// emit qualifies it identically (`user.ExtractedRecord`), so it collides with
/// the statically compiled declaration below — whose fields even match, which
/// is what would make the rebind silent. Only the compile-time heap section is
/// host-addressable; a runtime-compiled member lives on the moving heap and has
/// no codegen entry, exactly like an anonymous one.
#[tokio::test]
async fn a_runtime_compiled_class_instance_crosses_as_an_opaque_handle() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        /// The statically compiled declaration a rebind would land on.
        class ExtractedRecord {
            account: string,
        }

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format()}`
        }

        function main() -> unknown {
            let pkg = reflect.Package.compile({
                "schema.baml": "class ExtractedRecord { account string }"
            })
            let record_t = pkg.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"
            type Record = unreflect(record_t.as_type())
            Extract@parse<Record>(`{"account":"AC-1"}`)
        }
        "##
    );

    match output
        .result
        .expect("a runtime-compiled class instance should reach the host")
    {
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::RuntimeValue,
            ..
        }) => {}
        other => panic!(
            "a runtime-compiled class instance must cross as a tagged RuntimeValue, \
             not structurally under the static `ExtractedRecord`: {other:?}"
        ),
    }
}
