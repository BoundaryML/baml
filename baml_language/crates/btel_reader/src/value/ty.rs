//! A recorded type as structured JSON, the value of a `$type` envelope.
//!
//! The vocabulary is the playground's parameter schemas
//! (`baml_ide::param_schema`): a node's `type` names its kind (`{"type":
//! "list", "item": …}`), a nullable union folds to `optional`, and a named
//! part is `{name, schema}`. It is widened to every type a runtime value can
//! have: a generic class keeps its arguments, and functions, futures and
//! interfaces have their own kinds. A named type has its full name, as
//! `$class` spells it; a head the capture could not resolve has a `null` name.
//!
//! A class, enum or enum variant recorded with its definition names it with
//! `def`, after its arguments, and the cell it is rendered in may include the
//! definition there (see [`Definitions`]). A recorded definition's field
//! types use the same vocabulary, with `{"type": "typeParam", "index": N}`
//! for generic parameter `N`.

use baml_type::{Literal, TyTemplate};
use btel_snapshot::{
    CasId, DecodedDefinition, DefinitionHead, DefinitionRef, OwnedType, TypeIdentity,
    definition::DefinitionType,
};
use serde_json::{Map, Value as Json};

use super::envelope;

/// A head as rendered: its name, `None` when the capture could not resolve
/// it, and the definition it is recorded by.
#[derive(Clone, Debug)]
pub(super) struct Head {
    name: Option<String>,
    definition: Option<DefinitionRef>,
}

/// What rendering a type asks of the cell it is rendered into.
pub(super) trait Definitions {
    /// `node` names `definition`: add its `def`, and its `definition` when
    /// the cell includes it there.
    fn refer(&mut self, node: &mut Map<String, Json>, definition: DefinitionRef);
}

/// A definition's ID as rendered: its group's blob ID, and its position
/// there when it is not the first.
pub(super) fn definition_id(definition: DefinitionRef) -> String {
    use std::fmt::Write as _;
    let mut id = String::with_capacity(36);
    for byte in definition.group.as_bytes() {
        write!(id, "{byte:02x}").expect("write string");
    }
    if definition.member != 0 {
        write!(id, ".{}", definition.member).expect("write string");
    }
    id
}

/// Names definitions by ID alone.
struct Ids;
impl Definitions for Ids {
    fn refer(&mut self, node: &mut Map<String, Json>, definition: DefinitionRef) {
        node.insert("def".into(), Json::from(definition_id(definition)));
    }
}

/// A captured type, naming definitions by ID alone.
#[cfg(test)]
pub(super) fn json(ty: &OwnedType) -> Json {
    json_in(ty, &mut Ids)
}

/// A captured type rendered into a cell.
pub(super) fn json_in(ty: &OwnedType, cx: &mut impl Definitions) -> Json {
    let ty = TyTemplate::from(ty.map_heads(&mut |head| match head {
        TypeIdentity::Resolved(head) => Head {
            name: Some(head.name().to_string()),
            definition: None,
        },
        TypeIdentity::Unresolved(_) => Head {
            name: None,
            definition: None,
        },
        TypeIdentity::Defined(head) => Head {
            name: Some(head.name.to_string()),
            definition: Some(head.definition),
        },
    }));
    template(&ty, cx)
}

/// A field type of the definition group `group`, whose members are
/// `members`: a member is named by its position there.
pub(super) fn field_json(
    ty: &DefinitionType,
    group: CasId,
    members: &[DecodedDefinition],
    cx: &mut impl Definitions,
) -> Json {
    let ty = ty.map_heads(&mut |head| match head {
        DefinitionHead::Member(member) => Head {
            name: members
                .get(*member as usize)
                .map(|member| member.name().to_string()),
            definition: Some(DefinitionRef {
                group,
                member: *member,
            }),
        },
        DefinitionHead::Defined(head) => Head {
            name: Some(head.name.to_string()),
            definition: Some(head.definition),
        },
        DefinitionHead::Named(name) => Head {
            name: Some(name.to_string()),
            definition: None,
        },
    });
    template(&ty, cx)
}

fn template(ty: &TyTemplate<Head>, cx: &mut impl Definitions) -> Json {
    match ty {
        TyTemplate::Int => leaf("int"),
        TyTemplate::Bigint => leaf("bigint"),
        TyTemplate::Float => leaf("float"),
        TyTemplate::String => leaf("string"),
        TyTemplate::Bool => leaf("bool"),
        TyTemplate::Null => leaf("null"),
        TyTemplate::Uint8Array => leaf("uint8Array"),
        TyTemplate::Media(kind) => node("media", [("kind", Json::from(kind.tag_str()))]),
        TyTemplate::Literal(literal, _) => node("literal", [("value", literal_value(literal))]),
        TyTemplate::Class(head, args) => {
            let mut class = named("class", head);
            if !args.is_empty() {
                class.insert("args".into(), list(args, cx));
            }
            defined(class, head, cx)
        }
        TyTemplate::Interface(head, args, associated) => {
            let mut interface = named("interface", head);
            if !args.is_empty() {
                interface.insert("args".into(), list(args, cx));
            }
            if !associated.is_empty() {
                let bindings = associated
                    .iter()
                    .map(|(name, ty)| (name.to_string(), template(ty, cx)))
                    .collect();
                interface.insert("associated".into(), Json::Object(bindings));
            }
            Json::Object(interface)
        }
        TyTemplate::Enum(head) => defined(named("enum", head), head, cx),
        TyTemplate::EnumVariant(head, variant) => {
            let mut variant_ty = named("enumVariant", head);
            variant_ty.insert("value".into(), Json::from(variant.as_str()));
            defined(variant_ty, head, cx)
        }
        TyTemplate::TypeAlias(head) => Json::Object(named("alias", head)),
        TyTemplate::List(item) => node("list", [("item", template(item, cx))]),
        TyTemplate::Map { key, value } => node(
            "map",
            [("key", template(key, cx)), ("value", template(value, cx))],
        ),
        TyTemplate::Union(members) => {
            let is_null = |ty: &TyTemplate<Head>| matches!(ty, TyTemplate::Null);
            if !members.iter().any(is_null) {
                return node("union", [("variants", list(members, cx))]);
            }
            let rest: Vec<_> = members.iter().filter(|ty| !is_null(ty)).collect();
            match rest.as_slice() {
                [] => leaf("null"),
                [TyTemplate::Union(inner), ..] if rest.len() == 1 && inner.iter().any(is_null) => {
                    leaf("null")
                }
                [inner] => node("optional", [("inner", template(inner, cx))]),
                _ => {
                    let variants = rest.into_iter().map(|ty| template(ty, cx)).collect();
                    let union = node("union", [("variants", Json::Array(variants))]);
                    node("optional", [("inner", union)])
                }
            }
        }
        TyTemplate::Function {
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
                    part.insert("schema".into(), template(&param.ty, cx));
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
                    ("return", template(ret, cx)),
                    ("throws", template(throws, cx)),
                ],
            )
        }
        TyTemplate::Future(value, error) => node(
            "future",
            [
                ("value", template(value, cx)),
                ("error", template(error, cx)),
            ],
        ),
        TyTemplate::RustType => leaf("rustType"),
        TyTemplate::Type => leaf("type"),
        TyTemplate::Resource => leaf("resource"),
        TyTemplate::PromptAst => leaf("promptAst"),
        TyTemplate::Void => leaf("void"),
        TyTemplate::Unknown => leaf("unknown"),
        TyTemplate::Never => leaf("never"),
        TyTemplate::TypeArgRef(index) => node("typeParam", [("index", Json::from(*index))]),
        TyTemplate::AssociatedTypeProjection {
            base,
            interface,
            member,
        } => {
            let mut declaring = named("interface", &interface.name);
            if !interface.generics.is_empty() {
                declaring.insert("args".into(), list(&interface.generics, cx));
            }
            node(
                "projection",
                [
                    ("base", template(base, cx)),
                    ("interface", Json::Object(declaring)),
                    ("member", Json::from(member.as_str())),
                ],
            )
        }
    }
}

/// `node` with the definition its head is recorded by, if any.
fn defined(mut node: Map<String, Json>, head: &Head, cx: &mut impl Definitions) -> Json {
    if let Some(definition) = head.definition {
        cx.refer(&mut node, definition);
    }
    Json::Object(node)
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

fn named(name: &str, head: &Head) -> Map<String, Json> {
    let mut map = kind(name);
    map.insert(
        "name".into(),
        head.name.as_deref().map_or(Json::Null, Json::from),
    );
    map
}

fn list(types: &[TyTemplate<Head>], cx: &mut impl Definitions) -> Json {
    Json::Array(types.iter().map(|ty| template(ty, cx)).collect())
}

/// A literal's value as the value renderer writes it: a bigint as `$bigint`
/// digits, a float that JSON can't hold (`1e999`) as `$float` source text,
/// never a plain string a string literal could also be.
fn literal_value(literal: &Literal) -> Json {
    match literal {
        Literal::Int(n) => Json::from(*n),
        Literal::Bigint(n) => envelope("$bigint", Json::from(n.to_string())),
        Literal::Float(text) => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or_else(
                || envelope("$float", Json::from(text.as_str())),
                Json::Number,
            ),
        Literal::String(text) => Json::from(text.as_str()),
        Literal::Bool(b) => Json::Bool(*b),
    }
}

#[cfg(test)]
mod tests;
