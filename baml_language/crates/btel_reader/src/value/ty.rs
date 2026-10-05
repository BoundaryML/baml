//! A recorded type as structured JSON, the value of a `$type` envelope.
//!
//! The vocabulary is the playground's parameter schemas
//! (`baml_ide::param_schema`): a node's `type` names its kind (`{"type":
//! "list", "item": …}`), a nullable union folds to `optional`, and a named
//! part is `{name, schema}`. It is widened to every type a runtime value can
//! have: a generic class keeps its arguments, and functions, futures and
//! interfaces have their own kinds. A named type has its full name, as
//! `$class` spells it; a head the capture could not resolve has a `null` name.

use baml_type::{Literal, RealizedTy};
use btel_snapshot::{OwnedType, TypeIdentity};
use serde_json::{Map, Value as Json};

pub(super) fn json(ty: &OwnedType) -> Json {
    match ty {
        RealizedTy::Int => leaf("int"),
        RealizedTy::Bigint => leaf("bigint"),
        RealizedTy::Float => leaf("float"),
        RealizedTy::String => leaf("string"),
        RealizedTy::Bool => leaf("bool"),
        RealizedTy::Null => leaf("null"),
        RealizedTy::Uint8Array => leaf("uint8Array"),
        RealizedTy::Media(kind) => node("media", [("kind", Json::from(kind.tag_str()))]),
        RealizedTy::Literal(literal, _) => node("literal", [("value", literal_value(literal))]),
        RealizedTy::Class(head, args) => {
            let mut class = named("class", head);
            if !args.is_empty() {
                class.insert("args".into(), list(args));
            }
            Json::Object(class)
        }
        RealizedTy::Interface(head, args, associated) => {
            let mut interface = named("interface", head);
            if !args.is_empty() {
                interface.insert("args".into(), list(args));
            }
            if !associated.is_empty() {
                let bindings = associated
                    .iter()
                    .map(|(name, ty)| (name.to_string(), json(ty)))
                    .collect();
                interface.insert("associated".into(), Json::Object(bindings));
            }
            Json::Object(interface)
        }
        RealizedTy::Enum(head) => Json::Object(named("enum", head)),
        RealizedTy::EnumVariant(head, variant) => {
            let mut variant_ty = named("enumVariant", head);
            variant_ty.insert("value".into(), Json::from(variant.as_str()));
            Json::Object(variant_ty)
        }
        RealizedTy::TypeAlias(head) => Json::Object(named("alias", head)),
        RealizedTy::List(item) => node("list", [("item", json(item))]),
        RealizedTy::Map { key, value } => node("map", [("key", json(key)), ("value", json(value))]),
        RealizedTy::Union(members) => {
            if !ty.is_nullable_union() {
                return node("union", [("variants", list(members))]);
            }
            // `strip_null` leaves a union of only nulls unchanged.
            let inner = ty.strip_null();
            if inner.is_nullable_union() {
                leaf("null")
            } else {
                node("optional", [("inner", json(&inner))])
            }
        }
        RealizedTy::Function {
            params,
            ret,
            throws,
        } => {
            let params = params
                .iter()
                .map(|param| {
                    let mut part = Map::new();
                    part.insert(
                        "name".into(),
                        param
                            .name
                            .as_ref()
                            .map_or(Json::Null, |name| Json::from(name.as_str())),
                    );
                    part.insert("schema".into(), json(&param.ty));
                    if param.is_optional() {
                        part.insert("optional".into(), Json::Bool(true));
                    }
                    Json::Object(part)
                })
                .collect();
            node(
                "function",
                [
                    ("params", Json::Array(params)),
                    ("return", json(ret)),
                    ("throws", json(throws)),
                ],
            )
        }
        RealizedTy::Future(value, error) => {
            node("future", [("value", json(value)), ("error", json(error))])
        }
        RealizedTy::RustType => leaf("rustType"),
        RealizedTy::Type => leaf("type"),
        RealizedTy::Resource => leaf("resource"),
        RealizedTy::PromptAst => leaf("promptAst"),
        RealizedTy::Void => leaf("void"),
        RealizedTy::Unknown => leaf("unknown"),
        RealizedTy::Never => leaf("never"),
    }
}

fn kind(kind: &str) -> Map<String, Json> {
    let mut map = Map::new();
    map.insert("type".into(), Json::from(kind));
    map
}

fn leaf(name: &str) -> Json {
    Json::Object(kind(name))
}

fn node<const N: usize>(name: &str, parts: [(&str, Json); N]) -> Json {
    let mut map = kind(name);
    for (key, value) in parts {
        map.insert(key.into(), value);
    }
    Json::Object(map)
}

fn named(name: &str, head: &TypeIdentity) -> Map<String, Json> {
    let mut map = kind(name);
    let spelled = match head {
        TypeIdentity::Resolved(head) => Json::from(head.name().to_string()),
        TypeIdentity::Unresolved(_) => Json::Null,
    };
    map.insert("name".into(), spelled);
    map
}

fn list(types: &[OwnedType]) -> Json {
    Json::Array(types.iter().map(json).collect())
}

/// A literal's value as the value renderer writes it: a bigint as `$bigint`
/// digits, a float that JSON can't hold as its source text.
fn literal_value(literal: &Literal) -> Json {
    match literal {
        Literal::Int(n) => Json::from(*n),
        Literal::Bigint(n) => {
            let mut map = Map::new();
            map.insert("$bigint".into(), Json::from(n.to_string()));
            Json::Object(map)
        }
        Literal::Float(text) => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or_else(|| Json::from(text.as_str()), Json::Number),
        Literal::String(text) => Json::from(text.as_str()),
        Literal::Bool(b) => Json::Bool(*b),
    }
}

#[cfg(test)]
mod tests;
