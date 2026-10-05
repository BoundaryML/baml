use baml_type::{
    DeclarationName, Literal, MediaKind, Name, RealizedFunctionParamTy, RealizedTy, TaggedTypeName,
    TypeName, typetag::TypeTag,
};
use btel_snapshot::{OwnedType, TypeIdentity};
use serde_json::json;

use super::json as ty_json;

fn head(name: DeclarationName) -> TypeIdentity {
    TypeIdentity::Resolved(TaggedTypeName::new(TypeTag::from_i64(100), name))
}

fn user(name: &str) -> TypeIdentity {
    head(DeclarationName::Declared(TypeName::local(Name::new(name))))
}

fn class(name: &str, args: Vec<OwnedType>) -> OwnedType {
    RealizedTy::Class(user(name), args.into())
}

#[test]
fn data_types_use_the_playground_vocabulary() {
    // map<string, Resume[]>?
    let ty = RealizedTy::Union(
        [
            RealizedTy::Map {
                key: Box::new(RealizedTy::String),
                value: Box::new(RealizedTy::List(Box::new(class("Resume", vec![])))),
            },
            RealizedTy::Null,
        ]
        .into(),
    );
    assert_eq!(
        ty_json(&ty),
        json!({"type": "optional", "inner": {
            "type": "map",
            "key": {"type": "string"},
            "value": {"type": "list", "item": {"type": "class", "name": "user.Resume"}},
        }})
    );
    assert_eq!(
        ty_json(&RealizedTy::Union(
            [RealizedTy::Int, RealizedTy::String, RealizedTy::Null].into()
        )),
        json!({"type": "optional", "inner": {"type": "union", "variants": [
            {"type": "int"}, {"type": "string"},
        ]}})
    );
    assert_eq!(
        ty_json(&RealizedTy::Union(
            [RealizedTy::Int, RealizedTy::Bool].into()
        )),
        json!({"type": "union", "variants": [{"type": "int"}, {"type": "bool"}]})
    );
    assert_eq!(
        ty_json(&RealizedTy::Media(MediaKind::Image)),
        json!({"type": "media", "kind": "image"})
    );
}

#[test]
fn a_generic_class_keeps_its_arguments() {
    assert_eq!(
        ty_json(&class("Box", vec![RealizedTy::Int])),
        json!({"type": "class", "name": "user.Box", "args": [{"type": "int"}]})
    );
}

#[test]
fn named_types_have_their_full_names() {
    // A user type has its package, as a dependency's does; a runtime-created
    // type has no package, only its name; an unresolved head has none.
    let dependency = head(DeclarationName::Declared(TypeName::new(
        Name::new("baml"),
        vec![Name::new("json")],
        Name::new("Json"),
    )));
    assert_eq!(
        ty_json(&RealizedTy::TypeAlias(dependency)),
        json!({"type": "alias", "name": "baml.json.Json"})
    );
    assert_eq!(
        ty_json(&RealizedTy::Class(
            head(DeclarationName::Anonymous(Name::new("Person"))),
            Box::default()
        )),
        json!({"type": "class", "name": "Person"})
    );
    assert_eq!(
        ty_json(&RealizedTy::Enum(TypeIdentity::Unresolved(
            TypeTag::from_i64(101)
        ))),
        json!({"type": "enum", "name": null})
    );
    assert_eq!(
        ty_json(&RealizedTy::EnumVariant(
            user("Status"),
            Name::new("Active")
        )),
        json!({"type": "enumVariant", "name": "user.Status", "value": "Active"})
    );
}

#[test]
fn literals_hold_their_values() {
    let literal = |literal| ty_json(&RealizedTy::Literal(literal, baml_type::Freshness::Regular));
    assert_eq!(
        literal(Literal::String("hi".into())),
        json!({"type": "literal", "value": "hi"})
    );
    assert_eq!(
        literal(Literal::Int(3)),
        json!({"type": "literal", "value": 3})
    );
    assert_eq!(
        literal(Literal::Float("1.5".into())),
        json!({"type": "literal", "value": 1.5})
    );
    assert_eq!(
        literal(Literal::Bigint(12.into())),
        json!({"type": "literal", "value": {"$bigint": "12"}})
    );
}

#[test]
fn runtime_only_types_have_their_own_kinds() {
    // (x: int, label?: string) -> Future<Box<int>, never> throws Oops
    let function = RealizedTy::Function {
        params: [
            RealizedFunctionParamTy::required(Some(Name::new("x")), RealizedTy::Int),
            RealizedFunctionParamTy::optional(Some(Name::new("label")), RealizedTy::String),
            RealizedFunctionParamTy::required(None, RealizedTy::Bool),
        ]
        .into(),
        ret: Box::new(RealizedTy::Future(
            Box::new(class("Box", vec![RealizedTy::Int])),
            Box::new(RealizedTy::Never),
        )),
        throws: Box::new(class("Oops", vec![])),
    };
    assert_eq!(
        ty_json(&function),
        json!({
            "type": "function",
            "params": [
                {"name": "x", "schema": {"type": "int"}},
                {"name": "label", "schema": {"type": "string"}, "optional": true},
                {"name": null, "schema": {"type": "bool"}},
            ],
            "return": {
                "type": "future",
                "value": {"type": "class", "name": "user.Box", "args": [{"type": "int"}]},
                "error": {"type": "never"},
            },
            "throws": {"type": "class", "name": "user.Oops"},
        })
    );
    let interface = RealizedTy::Interface(
        user("Iterator"),
        [RealizedTy::Int].into(),
        [(Name::new("Item"), RealizedTy::String)].into(),
    );
    assert_eq!(
        ty_json(&interface),
        json!({
            "type": "interface",
            "name": "user.Iterator",
            "args": [{"type": "int"}],
            "associated": {"Item": {"type": "string"}},
        })
    );
    for (ty, kind) in [
        (RealizedTy::Type, "type"),
        (RealizedTy::Unknown, "unknown"),
        (RealizedTy::Void, "void"),
        (RealizedTy::Uint8Array, "uint8Array"),
        (RealizedTy::PromptAst, "promptAst"),
    ] {
        assert_eq!(ty_json(&ty), json!({ "type": kind }));
    }
}
