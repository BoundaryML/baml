//! Integration tests for the LLM `render_prompt` flow.
//!
//! These tests verify that:
//! 1. `get_jinja_template` returns the correct template for LLM functions
//! 2. `get_client` returns the correct client chain
//! 3. `render_prompt` correctly renders templates with arguments

use baml_builtins::{PromptAst as BuiltinPromptAst, PromptAstSimple};
use baml_type::TyAttr;
use bex_engine::{FunctionCallContextBuilder, Ty};
use bex_external_types::BexExternalAdt;
use bex_heap::{BexExternalValue, builtin_types::owned::LlmPrimitiveClient};

#[tokio::test]
async fn test_render_prompt_directly() {
    use indexmap::IndexMap;

    // Test the Jinja rendering directly
    let template = "Hello, {{ name }}! You are {{ age }} years old.";
    let mut args = IndexMap::new();
    args.insert(
        "name".to_string(),
        BexExternalValue::String("Alice".to_string()),
    );
    args.insert("age".to_string(), BexExternalValue::Int(30));

    let client = LlmPrimitiveClient {
        name: "test".to_string(),
        provider: "openai".to_string(),
        default_role: "user".to_string(),
        allowed_roles: vec![
            "user".to_string(),
            "assistant".to_string(),
            "system".to_string(),
        ],
        options: IndexMap::new(),
    };

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: sys_llm::OutputFormatContent::new(Ty::String {
            attr: TyAttr::default(),
        }),
        tags: IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let result = sys_llm::render_prompt(template, &args, &ctx).unwrap();

    match result {
        BuiltinPromptAst::Simple(s) => {
            assert_eq!(
                s,
                std::sync::Arc::new("Hello, Alice! You are 30 years old.".to_string().into())
            );
        }
        _ => panic!("Expected string result"),
    }
}

#[tokio::test]
async fn test_render_prompt_with_chat_roles() {
    use indexmap::IndexMap;

    let template = r#"
{{ _.role("system") }}
You are a helpful assistant.
{{ _.role("user") }}
{{ question }}
"#;
    let mut args = IndexMap::new();
    args.insert(
        "question".to_string(),
        BexExternalValue::String("What is 2+2?".to_string()),
    );

    let client = LlmPrimitiveClient {
        name: "test".to_string(),
        provider: "openai".to_string(),
        default_role: "user".to_string(),
        allowed_roles: vec![
            "user".to_string(),
            "assistant".to_string(),
            "system".to_string(),
        ],
        options: IndexMap::new(),
    };

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: sys_llm::OutputFormatContent::new(Ty::String {
            attr: TyAttr::default(),
        }),
        tags: IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let result = sys_llm::render_prompt(template, &args, &ctx).unwrap();

    // Result should be a Vec of messages
    match result {
        BuiltinPromptAst::Vec(messages) => {
            assert_eq!(messages.len(), 2);

            // Check first message (system)
            match messages[0].as_ref() {
                BuiltinPromptAst::Message { role, content, .. } => {
                    assert_eq!(role, "system");
                    match content.as_ref() {
                        PromptAstSimple::String(s) => {
                            assert_eq!(s, "You are a helpful assistant.");
                        }
                        _ => panic!("Expected string content"),
                    }
                }
                _ => panic!("Expected message"),
            }

            // Check second message (user)
            match messages[1].as_ref() {
                BuiltinPromptAst::Message { role, content, .. } => {
                    assert_eq!(role, "user");
                    match content.as_ref() {
                        PromptAstSimple::String(s) => {
                            assert_eq!(s, "What is 2+2?");
                        }
                        _ => panic!("Expected string content"),
                    }
                }
                _ => panic!("Expected message"),
            }
        }
        _ => panic!("Expected Vec of messages, got {result:?}"),
    }
}

#[tokio::test]
async fn test_render_prompt_with_enums() {
    use indexmap::IndexMap;
    use sys_llm::{RenderEnum, RenderEnumVariant};

    let template = "Category: {{ ctx.enums.Category.SPORTS }}";
    let args = IndexMap::new();

    let mut enums = std::collections::HashMap::new();
    enums.insert(
        "Category".to_string(),
        RenderEnum {
            name: "Category".to_string(),
            variants: vec![
                RenderEnumVariant {
                    name: "SPORTS".to_string(),
                },
                RenderEnumVariant {
                    name: "TECH".to_string(),
                },
                RenderEnumVariant {
                    name: "POLITICS".to_string(),
                },
            ],
        },
    );

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string()],
        },
        output_format: sys_llm::OutputFormatContent::new(Ty::String {
            attr: TyAttr::default(),
        }),
        tags: IndexMap::new(),
        enums,
    };

    let result = sys_llm::render_prompt(template, &args, &ctx).unwrap();

    match result {
        BuiltinPromptAst::Simple(s) => {
            let PromptAstSimple::String(s) = s.as_ref() else {
                panic!("Expected string content");
            };
            assert_eq!(s, "Category: SPORTS");
        }
        _ => panic!("Expected string result"),
    }
}

mod common;

// ============================================================================
// Full Engine Pipeline Tests (render_prompt, build_request, call_llm)
// ============================================================================

/// Test the full `render_prompt` flow through the engine.
#[tokio::test]
async fn test_render_prompt_e2e() {
    use bex_engine::{BexEngine, BexExternalValue};
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_render() -> int {
    let args = {};
    let result = baml.llm.render_prompt("Greet", args);
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "test_render",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    match result {
        Ok(value) => {
            assert_eq!(value, BexExternalValue::Int(42));
        }
        Err(e) => {
            panic!("test_render failed: {e}");
        }
    }
}

/// Test that `render_prompt` returns a `PromptAst` value.
#[tokio::test]
async fn test_render_prompt_returns_prompt_ast() {
    use bex_engine::{BexEngine, BexExternalValue};
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = { "name": "World" };
    baml.llm.render_prompt("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    match result {
        Ok(value) => match &value {
            BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) => match ast.as_ref() {
                BuiltinPromptAst::Simple(s) => {
                    let PromptAstSimple::String(s) = s.as_ref() else {
                        panic!("Expected string content");
                    };
                    assert_eq!(s, "Hello, World!");
                }
                _ => panic!("Expected simple content"),
            },
            other => {
                panic!("Expected Adt(PromptAst), got {other:?}");
            }
        },
        Err(e) => {
            panic!("get_prompt failed: {e}");
        }
    }
}

/// Test that `build_request` succeeds.
#[tokio::test]
async fn test_build_request_returns() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_build_request() -> int {
    let args = { "name": "World" };
    let request = baml.llm.build_request("Greet", args);
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "test_build_request",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;
    assert!(result.is_ok(), "build_request should succeed: {result:?}");
}

#[tokio::test]
async fn test_call_llm_function_string() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_call_llm() -> unknown {
    let args = { "name": "World" };
    baml.llm.call_llm_function("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "test_call_llm",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    assert!(result.is_err(), "Expected error without valid API key");
}

#[tokio::test]
async fn test_direct_llm_call() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_call_llm() -> string {
    Greet("World")
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "test_call_llm",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    assert!(result.is_err(), "Expected error without valid API key");
}

#[tokio::test]
async fn test_call_llm_function_non_string_returns_error() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> map<string, int> {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_call_llm() -> unknown {
    let args = { "name": "World" };
    baml.llm.call_llm_function("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "test_call_llm",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await;

    assert!(result.is_err(), "Expected error without valid API key");
}

// ============================================================================
// Output Format Tests (ctx.output_format)
// ============================================================================

#[tokio::test]
async fn test_output_format_class_return_type() {
    let actual = common::render_output_format(
        r#"
class Person {
    name string
    age int @description("Age in years")
}
"#,
        "Person",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  name: string,
  age: int, // Age in years
}"#
    );
}

#[tokio::test]
async fn test_output_format_enum_return_type() {
    let actual = common::render_output_format(
        r#"
enum Sentiment {
    POSITIVE @description("Happy or pleased")
    NEGATIVE
    NEUTRAL
}
"#,
        "Sentiment",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer with any of the categories:
Sentiment
----
- POSITIVE: Happy or pleased
- NEGATIVE
- NEUTRAL"#
    );
}

#[tokio::test]
async fn test_output_format_nested_class() {
    let actual = common::render_output_format(
        r#"
class Address {
    city string
    country string
}

class Person {
    name string
    address Address
}
"#,
        "Person",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  name: string,
  address: {
    city: string,
    country: string,
  },
}"#
    );
}

/// Test that `{{ ctx.output_format }}` renders nothing for string return type.
/// This test uses a custom template with markers to verify the format is truly empty.
#[tokio::test]
async fn test_output_format_string_return_type_is_empty() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello {{ name }}!OUTPUT_FORMAT_START{{ ctx.output_format }}OUTPUT_FORMAT_END
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = { "name": "World" };
    baml.llm.render_prompt("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("render_prompt with string return type failed");

    let actual = common::prompt_ast_to_string(&result);
    assert_eq!(actual, "Hello World!OUTPUT_FORMAT_STARTOUTPUT_FORMAT_END");
}

#[tokio::test]
async fn test_output_format_int_return_type() {
    let actual = common::render_output_format("", "int").await;
    assert_eq!(actual, "test\nAnswer as an int");
}

#[tokio::test]
async fn test_output_format_with_alias() {
    let actual = common::render_output_format(
        r#"
class User {
    first_name string @alias("firstName")
    last_name string @alias("lastName")
    age int
}
"#,
        "User",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  firstName: string,
  lastName: string,
  age: int,
}"#
    );
}

#[tokio::test]
async fn test_output_format_enum_skip_variant() {
    let actual = common::render_output_format(
        r#"
enum Status {
    ACTIVE
    INACTIVE
    DELETED @skip
}
"#,
        "Status",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer with any of the categories:
Status
----
- ACTIVE
- INACTIVE"#
    );
}

#[tokio::test]
async fn test_output_format_mutually_recursive_classes() {
    let actual = common::render_output_format(
        r#"
class Tree {
    data int
    children Forest
}

class Forest {
    trees Tree[]
}
"#,
        "Tree",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Forest {
  trees: Tree[],
}

Tree {
  data: int,
  children: Forest,
}

Answer in JSON using this schema: Tree"#
    );
}

#[tokio::test]
async fn test_output_format_recursive_alias_cycle() {
    let actual = common::render_output_format(
        r#"
type RecAliasOne = RecAliasTwo
type RecAliasTwo = RecAliasThree
type RecAliasThree = RecAliasOne[]
"#,
        "RecAliasOne",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
RecAliasOne = RecAliasTwo
RecAliasTwo = RecAliasThree
RecAliasThree = RecAliasOne[]

Answer in JSON using this schema: RecAliasOne"#
    );
}

#[tokio::test]
async fn test_output_format_recursive_map_alias() {
    let actual = common::render_output_format(
        "type RecursiveMapAlias = map<string, RecursiveMapAlias>",
        "RecursiveMapAlias",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
RecursiveMapAlias = map<string, RecursiveMapAlias>

Answer in JSON using this schema: RecursiveMapAlias"#
    );
}

#[tokio::test]
async fn test_output_format_recursive_list_alias() {
    let actual = common::render_output_format(
        "type RecursiveListAlias = RecursiveListAlias[]",
        "RecursiveListAlias",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
RecursiveListAlias = RecursiveListAlias[]

Answer in JSON using this schema: RecursiveListAlias"#
    );
}

#[tokio::test]
async fn test_output_format_recursive_union() {
    let actual = common::render_output_format(
        "type RecursiveUnion = string | map<string, RecursiveUnion>",
        "RecursiveUnion",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
RecursiveUnion = string or map<string, RecursiveUnion>

Answer in JSON using this schema: RecursiveUnion"#
    );
}

#[tokio::test]
async fn test_output_format_alias_to_recursive_class() {
    let actual = common::render_output_format(
        r#"
class LinkedListNode {
    data int
    next LinkedListNode?
}

type LinkedListAlias = LinkedListNode
"#,
        "LinkedListAlias",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
LinkedListNode {
  data: int,
  next: LinkedListNode or null,
}

Answer in JSON using this schema: LinkedListNode"#
    );
}

#[tokio::test]
async fn test_output_format_json_type_alias_cycle() {
    let actual = common::render_output_format(
        r#"
type JsonValue = int | float | string | bool | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]
"#,
        "JsonValue",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
JsonValue = int or float or string or bool or JsonObject or JsonArray
JsonArray = JsonValue[]
JsonObject = map<string, JsonValue>

Answer in JSON using this schema: JsonValue"#
    );
}

#[tokio::test]
async fn test_output_format_union_fields() {
    let actual = common::render_output_format(
        r#"
class TestUnion {
    value string | int | bool
    items (float | bool)[]
}
"#,
        "TestUnion",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  value: string or int or bool,
  items: (float or bool)[],
}"#
    );
}

#[tokio::test]
async fn test_output_format_class_description() {
    let actual = common::render_output_format(
        r#"
class Education {
    school string
    degree string
    year int

    @@description("Educational background of a person")
}
"#,
        "Education",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  // Educational background of a person

  school: string,
  degree: string,
  year: int,
}"#
    );
}

#[tokio::test]
async fn test_output_format_literal_int() {
    let actual = common::render_output_format("", "5").await;
    assert_eq!(actual, "test\nAnswer using this specific value:\n5");
}

#[tokio::test]
async fn test_output_format_literal_string() {
    let actual = common::render_output_format("", r#""hello""#).await;
    assert_eq!(actual, "test\nAnswer using this specific value:\n\"hello\"");
}

#[tokio::test]
async fn test_output_format_literal_bool() {
    let actual = common::render_output_format("", "true").await;
    assert_eq!(actual, "test\nAnswer using this specific value:\ntrue");
}

#[tokio::test]
async fn test_output_format_literal_union() {
    let actual = common::render_output_format("", r#"1 | true | "output""#).await;
    assert_eq!(
        actual,
        "test\nAnswer in JSON using any of these schemas:\n1 or true or \"output\""
    );
}

#[tokio::test]
async fn test_output_format_primitive_alias() {
    let actual =
        common::render_output_format("type Primitive = int | string | bool | float", "Primitive")
            .await;

    assert_eq!(
        actual,
        "test\nAnswer in JSON using any of these schemas:\nint or string or bool or float"
    );
}

#[tokio::test]
async fn test_output_format_map_alias() {
    let actual = common::render_output_format("type Graph = map<string, string[]>", "Graph").await;

    assert_eq!(
        actual,
        "test\nAnswer in JSON using this schema:\nmap<string, string[]>"
    );
}

#[tokio::test]
async fn test_output_format_optional_class() {
    let actual = common::render_output_format(
        r#"
class OptionalResult {
    name string
    value int?
}
"#,
        "OptionalResult?",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
{
  name: string,
  value: int or null,
} or null"#
    );
}

#[tokio::test]
async fn test_output_format_class_list() {
    let actual = common::render_output_format(
        r#"
class Item {
    name string
    price float
}
"#,
        "Item[]",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer with a JSON Array using this schema:
{
  name: string,
  price: float,
}[]"#
    );
}

// ============================================================================
// ctx.output_format Kwargs
// ============================================================================

#[tokio::test]
async fn test_output_format_prefix_null() {
    let actual = common::render_output_format_with_opts(
        r#"
enum Sentiment {
    POSITIVE
    NEGATIVE
    NEUTRAL
}
"#,
        "Sentiment",
        "prefix=null",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Sentiment
----
- POSITIVE
- NEGATIVE
- NEUTRAL"#
    );
}

#[tokio::test]
async fn test_output_format_map_style_type_parameters() {
    let actual = common::render_output_format_with_opts(
        r#"
class Recipe {
    name string
    ingredients map<string, string>
}
"#,
        "Recipe",
        "map_style='type_parameters'",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  name: string,
  ingredients: map<string, string>,
}"#
    );
}

#[tokio::test]
async fn test_output_format_or_splitter() {
    let actual = common::render_output_format_with_opts(
        r#"
class Result {
    value string | int
}
"#,
        "Result",
        "or_splitter=' | '",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
Answer in JSON using this schema:
{
  value: string | int,
}"#
    );
}

#[tokio::test]
async fn test_output_format_hoisted_class_prefix() {
    let actual = common::render_output_format_with_opts(
        r#"
class Node {
    data int
    next Node?
}
"#,
        "Node",
        "hoisted_class_prefix='type'",
    )
    .await;

    assert_eq!(
        actual,
        r#"test
type Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this type: Node"#
    );
}

// ============================================================================
// Template String Tests
// ============================================================================

/// Build a `BexExternalValue` wrapping a simple string `PromptAst`.
fn prompt_ast_string(s: &str) -> BexExternalValue {
    BexExternalValue::Adt(BexExternalAdt::PromptAst(std::sync::Arc::new(
        BuiltinPromptAst::Simple(std::sync::Arc::new(s.to_string().into())),
    )))
}

#[tokio::test]
async fn test_template_string_in_prompt() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

template_string Greet(name: string) #"Hello, {{ name }}!"#

function TestFunc(name: string) -> string {
    client TestClient
    prompt #"
        {{ Greet(name) }}
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = { "name": "Alice" };
    baml.llm.render_prompt("TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("failed to render prompt that calls template_string Greet(name)");
    assert_eq!(result, prompt_ast_string("Hello, Alice!"));
}

#[tokio::test]
async fn test_nested_template_strings() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

template_string Inner() #"INNER"#
template_string Outer() #"before {{ Inner() }} after"#

function TestFunc() -> string {
    client TestClient
    prompt #"{{ Outer() }}"#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = {};
    baml.llm.render_prompt("TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("failed to render prompt with nested template_strings Outer() -> Inner()");
    assert_eq!(result, prompt_ast_string("before INNER after"));
}

#[tokio::test]
async fn test_template_string_with_struct_arg() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

class Person {
    name string
    age int
}

template_string Describe(label: string, person: Person) #"{{ label }}: {{ person.name }} (age {{ person.age }})"#

function TestFunc(label: string, person: Person) -> string {
    client TestClient
    prompt #"
        {{ Describe(label, person) }}
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = { "label": "User", "person": { "name": "Bob", "age": 42 } };
    baml.llm.render_prompt("TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("failed to render prompt with 2-arg template_string Describe(label, person)");
    assert_eq!(result, prompt_ast_string("User: Bob (age 42)"));
}

#[tokio::test]
async fn test_parameterless_template_string() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

template_string Header() #"=== HEADER ==="#

function TestFunc() -> string {
    client TestClient
    prompt #"{{ Header() }}
Content here"#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = {};
    baml.llm.render_prompt("TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .expect("failed to render prompt that calls parameterless template_string Header()");
    assert_eq!(result, prompt_ast_string("=== HEADER ===\nContent here"));
}
