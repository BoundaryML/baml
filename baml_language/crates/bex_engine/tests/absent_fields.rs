//! Optional class fields a host leaves out of an argument.
//!
//! A Rust test because it is about host argument marshalling: BAML source
//! cannot build a `BexExternalValue` with a field missing. The BAML-side
//! behavior (constructors, JSON, `reflect.class.is_field_set`) is covered by
//! `crates/baml_tests/baml_src/ns_absent_fields/`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Patch {
    name: string?,
    expires_in: int?,
}

class Labeled {
    label: string,
    note: string?,
}

function encode(p: Patch) -> string {
    baml.json.to_string(p, omit_absent = true)
}

function encode_default(p: Patch) -> string {
    baml.json.to_string(p)
}

function name_is_set(p: Patch) -> bool {
    reflect.class.is_field_set(p, "name")
}

function echo(p: Patch) -> Patch {
    p
}

function label(l: Labeled) -> string {
    l.label
}

function main() -> int {
    0
}
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_for_engine(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .expect("engine"),
    )
}

async fn call(
    engine: &Arc<BexEngine>,
    function: &str,
    arg: BexExternalValue,
) -> Result<BexExternalValue, String> {
    engine
        .call_function(
            function,
            vec![arg],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .map_err(|error| error.to_string())
}

fn instance(class_name: &str, fields: &[(&str, BexExternalValue)]) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: class_name.to_string(),
        type_args: vec![],
        fields: fields
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.clone()))
            .collect(),
    }
}

fn host_map(fields: &[(&str, BexExternalValue)]) -> BexExternalValue {
    BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::unknown(),
        entries: fields
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.clone()))
            .collect::<IndexMap<_, _>>(),
    }
}

fn string(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

#[tokio::test]
async fn an_omitted_optional_field_arrives_absent() {
    let engine = engine();
    let omitted = instance("user.Patch", &[("name", string("x"))]);
    assert_eq!(
        call(&engine, "user.encode", omitted.clone()).await,
        Ok(string(r#"{"name":"x"}"#)),
    );
    // The default encoding is unchanged: an absent field is written as null.
    assert_eq!(
        call(&engine, "user.encode_default", omitted).await,
        Ok(string(r#"{"name":"x","expires_in":null}"#)),
    );
    assert_eq!(
        call(&engine, "user.name_is_set", instance("user.Patch", &[])).await,
        Ok(BexExternalValue::Bool(false)),
    );
}

#[tokio::test]
async fn an_explicit_null_field_arrives_null() {
    let engine = engine();
    let explicit = instance(
        "user.Patch",
        &[
            ("name", BexExternalValue::Null),
            ("expires_in", BexExternalValue::Null),
        ],
    );
    assert_eq!(
        call(&engine, "user.encode", explicit.clone()).await,
        Ok(string(r#"{"name":null,"expires_in":null}"#)),
    );
    assert_eq!(
        call(&engine, "user.name_is_set", explicit).await,
        Ok(BexExternalValue::Bool(true)),
    );
}

#[tokio::test]
async fn a_plain_host_object_follows_the_same_rule() {
    let engine = engine();
    assert_eq!(
        call(
            &engine,
            "user.encode",
            host_map(&[("expires_in", BexExternalValue::Null)])
        )
        .await,
        Ok(string(r#"{"expires_in":null}"#)),
    );
}

#[tokio::test]
async fn an_omitted_required_field_is_still_rejected() {
    let engine = engine();
    let error = call(
        &engine,
        "user.label",
        instance("user.Labeled", &[("note", string("n"))]),
    )
    .await
    .expect_err("`label` is required");
    assert!(
        error.contains("Missing field `label`"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn an_absent_field_leaves_the_engine_as_null() {
    // Host decoders (Go, C++) require every declared field, so the outbound
    // value still carries an absent field as an explicit null.
    let engine = engine();
    let returned = call(
        &engine,
        "user.echo",
        instance("user.Patch", &[("name", string("x"))]),
    )
    .await
    .expect("echo");
    let BexExternalValue::Instance { fields, .. } = returned else {
        panic!("expected an instance, got {returned:?}");
    };
    assert_eq!(fields.get("expires_in"), Some(&BexExternalValue::Null));
    assert_eq!(fields.get("name"), Some(&string("x")));
}
