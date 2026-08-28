mod common;

use std::sync::Arc;

use baml_db::testing::compile_multi_file;
use bex_engine::{
    BexCallArg, BexEngine, BexExternalAdt, BexExternalValue, FunctionCallContextBuilder,
    FunctionOperation,
};
use bex_external_types::TaggedHeapHandleKind;
use sys_native::SysOpsExt as _;

const SOURCE: &str = r#"
    class RuntimeOutput {
        compiled_marker int
    }

    function Echo(value: string) -> string { value }

    function Ask(question: string) -> string {
        client: "openai/gpt-4o-mini"
        prompt: `${question}`
    }

    function Dollar$Ask(question: string) -> string {
        client: "openai/gpt-4o-mini"
        prompt: `${question}`
    }

    function DynamicClientSelector() -> string {
        "openai/gpt-4o-mini"
    }

    function DynamicClientAsk(question: string) -> string {
        client: DynamicClientSelector()
        prompt: `${question}`
    }

    class SpecRunner {
        prefix string

        function Ask(self, question: string) -> string {
            client: "openai/gpt-4o-mini"
            prompt: `${self.prefix}: ${question}`
        }

        function StaticAsk(question: string) -> string {
            client: "openai/gpt-4o-mini"
            prompt: `static: ${question}`
        }
    }

    function BoundMethodPromptText(question: string) -> string {
        let runner = SpecRunner { prefix: "bound" }
        runner.Ask@spec(question).prompt().text()
    }

    function StaticMethodPromptText(question: string) -> string {
        SpecRunner.StaticAsk@spec(question).prompt().text()
    }

    function DynamicClientSpecId() -> string {
        DynamicClientAsk@spec("schema only").default_client.id()
    }

    function AskSpecValue() -> unknown {
        Ask@spec
    }

    function AskValue() -> unknown {
        Ask
    }

    function BoundAskValue() -> unknown {
        let runner = SpecRunner { prefix: "boundary-bound" }
        runner.Ask
    }

    function StaticAskValue() -> unknown {
        SpecRunner.StaticAsk
    }

    function AskPromptText(question: string) -> string {
        Ask@spec(question).prompt().text()
    }

    function AskRequestMethod(question: string) -> string {
        Ask@spec(question).build_request().method
    }

    function AskParse(raw: string) -> string {
        Ask@spec("schema only").parse(raw)
    }

    function AskClientId() -> string {
        Ask@spec("schema only").client_id()
    }

    function GenericAsk<T>(question: string) -> T {
        client: "openai/gpt-4o-mini"
        prompt: `${question} ${ctx.output_format()}`
    }

    function GenericAskParse(raw: string) -> string {
        GenericAsk@spec<string>("schema only").parse(raw)
    }

    function GenericAskValue() -> unknown {
        GenericAsk<string>
    }

    function StringSpecPromptText(spec: ai.FunctionSpec<string>) -> string {
        spec.prompt().text()
    }

    function ParseStringSpec(spec: ai.FunctionSpec<string>, raw: string) -> string {
        spec.parse(raw)
    }

    function DynamicGenericAskOutputType() -> string {
        let output_type = reflect.class.new("RuntimeOutput", {
            "value": reflect.Type.of<string>(),
        }).as_type();
        type OutputType = unreflect(output_type);
        GenericAsk@spec<OutputType>("schema only").output_type().to_string()
    }

    function DynamicGenericAskSpec() -> unknown {
        let output_type = reflect.class.new("RuntimeOutput", {
            "value": reflect.Type.of<string>(),
        }).as_type();
        type OutputType = unreflect(output_type);
        GenericAsk@spec<OutputType>("schema only")
    }
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            common::compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("test engine"),
    )
}

fn capability_collision_engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_multi_file(&[
                (
                    "ns_ai/function_spec.baml",
                    r#"
class FunctionSpec {
    marker string
}

function MakeFunctionSpecCollision() -> FunctionSpec {
    FunctionSpec { marker: "compiled-function-spec" }
}

function ReadFunctionSpecCollision(value: FunctionSpec) -> string {
    value.marker
}
"#,
                ),
                (
                    "ns_ai/ns_stream/stream.baml",
                    r#"
class Stream {
    marker string
}

function MakeStreamCollision() -> Stream {
    Stream { marker: "compiled-stream" }
}

function ReadStreamCollision(value: Stream) -> string {
    value.marker
}
"#,
                ),
            ]),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("capability collision test engine"),
    )
}

fn args(engine: &BexEngine, function_name: &str) -> Vec<BexCallArg> {
    engine
        .function_params(function_name)
        .expect("function params")
        .into_iter()
        .map(|(name, _, has_default)| match name {
            "question" | "value" => {
                BexCallArg::Provided(Box::new(BexExternalValue::String("hi".into())))
            }
            _ if has_default => BexCallArg::OmittedDefault,
            _ => panic!("unexpected required parameter `{name}`"),
        })
        .collect()
}

fn operation_args(
    engine: &BexEngine,
    function_name: &str,
    operation: FunctionOperation,
) -> Vec<BexCallArg> {
    engine
        .function_operation_params(function_name, operation)
        .expect("operation params")
        .into_iter()
        .map(|(name, _, has_default)| match name {
            "question" | "value" => {
                BexCallArg::Provided(Box::new(BexExternalValue::String("hi".into())))
            }
            _ if has_default => BexCallArg::OmittedDefault,
            _ => panic!("unexpected required parameter `{name}`"),
        })
        .collect()
}

async fn returned_callable(
    engine: &Arc<BexEngine>,
    function_name: &str,
) -> bex_external_types::Handle {
    let returned = engine
        .call_function(
            function_name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap_or_else(|error| panic!("{function_name} failed: {error}"));
    let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
        kind: TaggedHeapHandleKind::Callable,
        heap_handle,
        ..
    }) = returned
    else {
        panic!("{function_name} must return a callable handle");
    };
    heap_handle
}

async fn callable_spec(
    engine: &Arc<BexEngine>,
    value_function: &str,
    question: &str,
) -> BexExternalValue {
    let handle = returned_callable(engine, value_function).await;
    engine
        .call_callable_operation_named(
            handle,
            FunctionOperation::Spec,
            indexmap::IndexMap::from([(
                "question".to_string(),
                BexExternalValue::String(question.into()),
            )]),
            indexmap::IndexMap::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap_or_else(|error| panic!("{value_function} Spec operation failed: {error}"))
}

#[tokio::test]
async fn dynamic_client_expression_keeps_source_and_boundary_operations() {
    let engine = engine();

    let client_id = engine
        .call_function(
            "DynamicClientSpecId",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("source @spec with a dynamic client expression");
    assert_eq!(
        client_id,
        BexExternalValue::String("openai/gpt-4o-mini".into())
    );

    let spec = engine
        .call_function_bound_args_operation(
            "DynamicClientAsk",
            FunctionOperation::Spec,
            operation_args(&engine, "DynamicClientAsk", FunctionOperation::Spec),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("boundary Spec operation with a dynamic client expression");
    assert!(matches!(
        spec,
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::FunctionSpec,
            ..
        })
    ));

    let stream_params = engine
        .function_operation_params("DynamicClientAsk", FunctionOperation::Stream)
        .expect("boundary Stream entry with a dynamic client expression");
    assert_eq!(
        stream_params
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>(),
        ["question", "client", "on_event"]
    );
}

#[tokio::test]
async fn operation_entries_use_exact_private_companion_names() {
    let engine = engine();
    assert!(!engine.function_exists("Ask$spec"));
    assert!(!engine.function_exists("Ask$stream"));
    assert!(engine.function_exists("Ask@spec"));
    assert!(engine.function_exists("Ask@stream"));
    assert!(engine.function_exists("Dollar$Ask@spec"));
    assert!(engine.function_exists("Dollar$Ask@stream"));
    assert!(engine.function_exists("SpecRunner.StaticAsk@spec"));
    assert!(engine.function_exists("SpecRunner.StaticAsk@stream"));

    let spec_params = engine
        .function_operation_params("Ask", FunctionOperation::Spec)
        .expect("spec operation params");
    assert_eq!(
        spec_params
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>(),
        ["question"]
    );

    let spec = engine
        .call_function_bound_args_operation(
            "Ask",
            FunctionOperation::Spec,
            operation_args(&engine, "Ask", FunctionOperation::Spec),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("spec operation");
    assert!(matches!(
        spec,
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::FunctionSpec,
            ..
        })
    ));

    let static_spec = engine
        .call_function_bound_args_operation(
            "SpecRunner.StaticAsk",
            FunctionOperation::Spec,
            operation_args(&engine, "SpecRunner.StaticAsk", FunctionOperation::Spec),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("qualified static-method spec operation");
    assert!(matches!(
        static_spec,
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::FunctionSpec,
            ..
        })
    ));

    let stream_params = engine
        .function_operation_params("Ask", FunctionOperation::Stream)
        .expect("stream operation params");
    assert_eq!(
        stream_params
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>(),
        ["question", "client", "on_event"]
    );
    for companion in ["Ask@spec", "Ask@stream"] {
        assert!(
            engine.find_user_function(companion).is_none(),
            "compiler-private companion `{companion}` must not be a CLI/user entry point"
        );
    }
    assert!(
        engine
            .user_functions()
            .iter()
            .all(|function| !function.display_name.ends_with("@spec")
                && !function.display_name.ends_with("@stream")),
        "compiler-private companions must not appear in function listings"
    );

    let dollar_spec_params = engine
        .function_operation_params("Dollar$Ask", FunctionOperation::Spec)
        .expect("a legal authored `$` name keeps its Spec operation");
    assert_eq!(
        dollar_spec_params
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>(),
        ["question"]
    );

    let dollar_stream_params = engine
        .function_operation_params("Dollar$Ask", FunctionOperation::Stream)
        .expect("a legal authored `$` name keeps its Stream operation");
    assert_eq!(
        dollar_stream_params
            .iter()
            .map(|(name, _, _)| *name)
            .collect::<Vec<_>>(),
        ["question", "client", "on_event"]
    );

    let error = engine
        .call_function_bound_args_operation(
            "Echo",
            FunctionOperation::Spec,
            args(&engine, "Echo"),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect_err("plain function must not expose a spec operation")
        .to_string();
    assert!(error.contains("does not support the `spec` operation"));

    let error = engine
        .call_function_bound_args_operation(
            "Echo",
            FunctionOperation::Stream,
            args(&engine, "Echo"),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect_err("plain function must not expose a stream operation")
        .to_string();
    assert!(error.contains("does not support the `stream` operation"));
}

#[tokio::test]
async fn returned_spec_companion_callable_invokes_directly() {
    let engine = engine();
    let returned = engine
        .call_function(
            "AskSpecValue",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("return spec companion callable");
    let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
        kind: TaggedHeapHandleKind::Callable,
        heap_handle,
        ..
    }) = returned
    else {
        panic!("expected callable handle");
    };

    let spec = engine
        .call_callable(
            heap_handle,
            vec![BexExternalValue::String("hi".into())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("spec companion callable must invoke directly");
    assert!(matches!(
        spec,
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::FunctionSpec,
            ..
        })
    ));
}

#[tokio::test]
async fn callable_spec_operation_preserves_generic_and_method_context() {
    let engine = engine();

    for (value_function, question, expected_prompt) in [
        ("AskValue", "named", "named"),
        ("BoundAskValue", "method", "boundary-bound: method"),
        ("StaticAskValue", "static", "static: static"),
    ] {
        let spec = callable_spec(&engine, value_function, question).await;
        let prompt = engine
            .call_function(
                "StringSpecPromptText",
                vec![spec],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .unwrap_or_else(|error| panic!("prompt from {value_function} failed: {error}"));
        let BexExternalValue::String(prompt) = prompt else {
            panic!("prompt from {value_function} must be a string");
        };
        assert!(prompt.contains(expected_prompt), "{prompt}");
    }

    let generic = callable_spec(&engine, "GenericAskValue", "generic").await;
    let parsed = engine
        .call_function(
            "ParseStringSpec",
            vec![generic, BexExternalValue::String("\"typed\"".into())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("generic callable Spec must preserve the string type argument");
    assert_eq!(parsed, BexExternalValue::String("typed".into()));
}

#[tokio::test]
async fn source_spec_methods_use_the_private_companion() {
    let engine = engine();
    let call = |name: &'static str, value: &'static str| {
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    name,
                    vec![BexExternalValue::String(value.into())],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
                .unwrap_or_else(|error| panic!("{name} failed: {error}"))
        }
    };

    let BexExternalValue::String(prompt) = call("AskPromptText", "hello").await else {
        panic!("prompt text must be a string");
    };
    assert!(prompt.contains("hello"));

    let BexExternalValue::String(prompt) = call("BoundMethodPromptText", "hello").await else {
        panic!("bound method prompt text must be a string");
    };
    assert!(prompt.contains("bound: hello"));

    let BexExternalValue::String(prompt) = call("StaticMethodPromptText", "hello").await else {
        panic!("static method prompt text must be a string");
    };
    assert!(prompt.contains("static: hello"));

    assert_eq!(
        call("AskRequestMethod", "hello").await,
        BexExternalValue::String("POST".into())
    );
    assert_eq!(
        call("AskParse", "\"parsed\"").await,
        BexExternalValue::String("parsed".into())
    );
    assert_eq!(
        call("GenericAskParse", "\"generic\"").await,
        BexExternalValue::String("generic".into())
    );
    assert!(matches!(
        engine
            .call_function(
                "AskClientId",
                Vec::new(),
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .expect("client_id"),
        BexExternalValue::String(_)
    ));
    assert_eq!(
        engine
            .call_function(
                "DynamicGenericAskOutputType",
                Vec::new(),
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .expect("dynamic generic output_type"),
        BexExternalValue::String("RuntimeOutput".into())
    );
}

#[tokio::test]
async fn trusted_spec_receiver_uses_live_type_identity_across_host_roundtrip() {
    let engine = engine();
    let spec = engine
        .call_function(
            "DynamicGenericAskSpec",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("dynamic generic spec");
    assert!(matches!(
        spec,
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::FunctionSpec,
            ..
        })
    ));
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;

    let client = engine
        .call_function(
            "ai.FunctionSpec.client_id",
            vec![spec],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("live receiver type args must not rebind by colliding name");
    assert!(matches!(client, BexExternalValue::String(_)));
}

#[tokio::test]
async fn compiled_classes_with_capability_display_names_stay_structural() {
    let engine = capability_collision_engine();
    for (make, read, class_name, marker) in [
        (
            "user.ai.MakeFunctionSpecCollision",
            "user.ai.ReadFunctionSpecCollision",
            "user.ai.FunctionSpec",
            "compiled-function-spec",
        ),
        (
            "user.ai.stream.MakeStreamCollision",
            "user.ai.stream.ReadStreamCollision",
            "user.ai.stream.Stream",
            "compiled-stream",
        ),
    ] {
        let value = engine
            .call_function(
                make,
                Vec::new(),
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .unwrap_or_else(|error| panic!("{make} failed: {error}"));
        assert!(
            matches!(
                &value,
                BexExternalValue::Instance {
                    class_name: actual,
                    ..
                } if actual == class_name
            ),
            "{class_name} must not impersonate a stdlib capability: {value:?}",
        );

        let round_tripped = engine
            .call_function(
                read,
                vec![value],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .unwrap_or_else(|error| panic!("host pass-back into {read} failed: {error}"));
        assert_eq!(round_tripped, BexExternalValue::String(marker.into()));
    }
}
