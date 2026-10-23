//! Host class arguments that leave out a field.
//!
//! A Rust test because the subject is host argument marshalling: BAML source
//! cannot build a class value with a field missing, only a host can.
//!
//! An omitted field whose declared type admits `null` is read as `null`, the
//! same as if the host had sent `null`. An omitted required field is still a
//! type error, and so is an omitted nullable field when the value carries a
//! key the class does not declare.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    class Resume {
        name string
    }

    type MaybeText = string | null
    type Chain = Chain[] | null

    class Client {
        base_url string
        api_key string
        workspace string?
        retries int | null
        owner Resume?
        note MaybeText
        chain Chain
        extra json
    }

    class Tagged {
        id string
        ws string? @alias("workspace_id")
    }

    class Endpoint {
        url string
        workspace string?
    }

    class Region {
        url string
        region string
    }

    class Note {
        text string
        meta json
    }

    class Tag {
        text string
    }

    function workspace_of(c: Client) -> string? { c.workspace }
    function retries_of(c: Client) -> int? { c.retries }
    function owner_of(c: Client) -> Resume? { c.owner }
    function note_of(c: Client) -> string? { c.note }
    function chain_of(c: Client) -> Chain { c.chain }
    function extra_of(c: Client) -> json { c.extra }
    function base_url_of(c: Client) -> string { c.base_url }
    function echo(c: Client) -> Client { c }
    function echo_all(cs: Client[]) -> Client[] { cs }
    function ws_of(t: Tagged) -> string? { t.ws }

    function pick(target: Endpoint | Region) -> string {
        match (target) {
            let e: Endpoint => "endpoint",
            let r: Region => "region",
        }
    }

    function save(item: Note | Tag) -> string {
        match (item) {
            let n: Note => "note",
            let t: Tag => "tag",
        }
    }

    function main() -> int { 0 }
"#;

async fn call(
    function: &str,
    args: Vec<BexExternalValue>,
) -> Result<BexExternalValue, EngineError> {
    let engine = Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("engine"),
    );
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// A plain host object (a TypeScript literal, a Python dict): no class name,
/// just the keys the host chose to send.
fn plain_object(fields: &[(&str, BexExternalValue)]) -> BexExternalValue {
    BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::unknown(),
        entries: fields
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect(),
    }
}

fn required_only() -> Vec<(&'static str, BexExternalValue)> {
    vec![
        (
            "base_url",
            BexExternalValue::String("http://127.0.0.1:1".into()),
        ),
        ("api_key", BexExternalValue::String("k".into())),
    ]
}

/// `required_only()` plus an explicit `null` for every nullable field.
fn explicit_nulls() -> Vec<(&'static str, BexExternalValue)> {
    let mut fields = required_only();
    for name in ["workspace", "retries", "owner", "note", "chain", "extra"] {
        fields.push((name, BexExternalValue::Null));
    }
    fields
}

fn unwrap_union(value: BexExternalValue) -> BexExternalValue {
    match value {
        BexExternalValue::Union { value, .. } => unwrap_union(*value),
        other => other,
    }
}

#[tokio::test]
async fn omitted_optional_field_reads_as_null() {
    let out = call("workspace_of", vec![plain_object(&required_only())])
        .await
        .expect("omitted `string?` field is null");
    assert_eq!(unwrap_union(out), BexExternalValue::Null);
}

#[tokio::test]
async fn omitted_nullable_union_class_and_alias_fields_read_as_null() {
    // `Chain` and `json` are recursive aliases, so they reach the engine as
    // alias references rather than expanded unions.
    for function in ["retries_of", "owner_of", "note_of", "chain_of", "extra_of"] {
        let out = call(function, vec![plain_object(&required_only())])
            .await
            .unwrap_or_else(|e| panic!("{function}: omitted nullable field is null, got {e}"));
        assert_eq!(unwrap_union(out), BexExternalValue::Null, "{function}");
    }
}

#[tokio::test]
async fn omitted_field_matches_explicit_null() {
    let omitted = call("echo", vec![plain_object(&required_only())])
        .await
        .expect("omitted");
    let explicit = call("echo", vec![plain_object(&explicit_nulls())])
        .await
        .expect("explicit null");
    assert_eq!(omitted, explicit);
    let BexExternalValue::Instance { fields, .. } = unwrap_union(omitted) else {
        panic!("expected a Client instance");
    };
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "base_url",
            "api_key",
            "workspace",
            "retries",
            "owner",
            "note",
            "chain",
            "extra"
        ],
    );
}

#[tokio::test]
async fn omitted_optional_field_on_named_instance_reads_as_null() {
    let fields: IndexMap<&str, BexExternalValue> = required_only().into_iter().collect();
    let out = call(
        "workspace_of",
        vec![BexExternalValue::instance("user.Client", fields)],
    )
    .await
    .expect("named instance with an omitted `string?` field");
    assert_eq!(unwrap_union(out), BexExternalValue::Null);
}

#[tokio::test]
async fn supplied_optional_field_is_kept() {
    let mut fields = required_only();
    fields.push(("workspace", BexExternalValue::String("ws".into())));
    let out = call("workspace_of", vec![plain_object(&fields)])
        .await
        .expect("supplied");
    assert_eq!(unwrap_union(out), BexExternalValue::String("ws".into()));
}

#[tokio::test]
async fn omitted_required_field_is_still_an_error() {
    let err = call(
        "base_url_of",
        vec![plain_object(&[(
            "base_url",
            BexExternalValue::String("http://127.0.0.1:1".into()),
        )])],
    )
    .await
    .expect_err("`api_key` is required");
    let message = err.to_string();
    assert!(
        message.contains("Missing field `api_key` in external Instance for class `user.Client`"),
        "unexpected error: {message}"
    );
}

#[tokio::test]
async fn omitted_fields_in_a_list_of_plain_objects_read_as_null() {
    let list = |item: BexExternalValue| BexExternalValue::Array {
        element_type: baml_type::RuntimeTy::unknown(),
        items: vec![item.clone(), item],
    };
    let omitted = call("echo_all", vec![list(plain_object(&required_only()))])
        .await
        .expect("omitted");
    let explicit = call("echo_all", vec![list(plain_object(&explicit_nulls()))])
        .await
        .expect("explicit null");
    assert_eq!(omitted, explicit);
}

#[tokio::test]
async fn omitted_nullable_field_next_to_an_undeclared_key_is_an_error() {
    // A misspelled key, or a field's `@alias` name (materialization reads
    // declared names only), must not leave the real field a silent `null`.
    let mut misspelled = required_only();
    misspelled.push(("workspce", BexExternalValue::String("ws".into())));
    let aliased = vec![
        ("id", BexExternalValue::String("1".into())),
        ("workspace_id", BexExternalValue::String("ws".into())),
    ];
    for (function, fields, expected) in [
        (
            "workspace_of",
            misspelled,
            "Missing field `workspace` in external Instance for class `user.Client` \
             (the value has undeclared key `workspce`)",
        ),
        (
            "ws_of",
            aliased,
            "Missing field `ws` in external Instance for class `user.Tagged` \
             (the value has undeclared key `workspace_id`)",
        ),
    ] {
        let err = call(function, vec![plain_object(&fields)])
            .await
            .expect_err(function);
        let message = err.to_string();
        assert!(message.contains(expected), "{function}: {message}");
    }
}

#[tokio::test]
async fn omitted_nullable_field_still_selects_the_class_arm_of_a_union() {
    let url = || ("url", BexExternalValue::String("http://127.0.0.1:1".into()));
    let out = call("pick", vec![plain_object(&[url()])])
        .await
        .expect("only `Endpoint` admits a payload without `region`");
    assert_eq!(out, BexExternalValue::String("endpoint".into()));

    let region = ("region", BexExternalValue::String("eu".into()));
    let out = call("pick", vec![plain_object(&[url(), region])])
        .await
        .expect("`region` is not an `Endpoint` field");
    assert_eq!(out, BexExternalValue::String("region".into()));
}

#[tokio::test]
async fn omitted_alias_nullable_field_does_not_widen_union_arm_selection() {
    // Arm selection for a plain object only treats a direct `T | null` field
    // as omittable, so `Note` (whose `meta` is `json`) does not match and the
    // payload is not ambiguous between `Note` and `Tag`.
    let out = call(
        "save",
        vec![plain_object(&[(
            "text",
            BexExternalValue::String("hi".into()),
        )])],
    )
    .await
    .expect("only `Tag` matches");
    assert_eq!(out, BexExternalValue::String("tag".into()));
}
