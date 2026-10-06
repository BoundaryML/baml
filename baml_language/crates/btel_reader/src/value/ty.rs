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
//! A class or enum whose definition was recorded also carries its definition
//! id (`def`), and the first reference to it in a rendered cell carries the
//! definition itself (`definition`). A definition's field types render the
//! same way, with a generic parameter as `{"type": "typeParam", "index": N}`.

use std::sync::Arc;

use baml_type::{Literal, TyTemplate, typetag::TypeTag};
use btel_snapshot::{OwnedType, TypeIdentity};
use btel_types::{DefinitionHead, TypeDeclaration};
use serde_json::{Map, Value as Json};

use super::envelope;

/// Definitions a rendered cell refers to. `reference` returns a declaration
/// only for the first reference in the cell, and marks it before the caller
/// renders it, so a recursive reference inside it is the id alone.
pub(super) trait CellDefinitions {
    fn reference(&mut self, tag: TypeTag) -> Option<(Arc<str>, Option<Arc<TypeDeclaration>>)>;
}

#[cfg(test)]
pub(super) struct NoDefinitions;

#[cfg(test)]
impl CellDefinitions for NoDefinitions {
    fn reference(&mut self, _: TypeTag) -> Option<(Arc<str>, Option<Arc<TypeDeclaration>>)> {
        None
    }
}

/// A type head as rendered: its identity, if definitions can refer to it,
/// and its spelling.
trait Head: Clone {
    fn tag(&self) -> Option<TypeTag>;
    fn spelling(&self) -> Json;
}

impl Head for TypeIdentity {
    fn tag(&self) -> Option<TypeTag> {
        match self {
            TypeIdentity::Resolved(head) => Some(head.tag()),
            TypeIdentity::Unresolved(_) => None,
        }
    }
    fn spelling(&self) -> Json {
        match self {
            TypeIdentity::Resolved(head) => Json::from(head.name().to_string()),
            TypeIdentity::Unresolved(_) => Json::Null,
        }
    }
}

impl Head for DefinitionHead {
    fn tag(&self) -> Option<TypeTag> {
        Some(self.tag)
    }
    fn spelling(&self) -> Json {
        self.name
            .as_ref()
            .map_or(Json::Null, |name| Json::from(name.to_string()))
    }
}

#[cfg(test)]
pub(super) fn json(ty: &OwnedType) -> Json {
    json_with(ty, &mut NoDefinitions)
}

pub(super) fn json_with(ty: &OwnedType, defs: &mut dyn CellDefinitions) -> Json {
    template(&TyTemplate::from(ty), defs)
}

fn template<H: Head>(ty: &TyTemplate<H>, defs: &mut dyn CellDefinitions) -> Json {
    let list = |types: &[TyTemplate<H>], defs: &mut dyn CellDefinitions| {
        Json::Array(types.iter().map(|ty| template(ty, defs)).collect())
    };
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
                class.insert("args".into(), list(args, defs));
            }
            reference(&mut class, "def", "definition", head.tag(), defs);
            Json::Object(class)
        }
        TyTemplate::Interface(head, args, associated) => {
            let mut interface = named("interface", head);
            if !args.is_empty() {
                interface.insert("args".into(), list(args, defs));
            }
            if !associated.is_empty() {
                let bindings = associated
                    .iter()
                    .map(|(name, ty)| (name.to_string(), template(ty, defs)))
                    .collect();
                interface.insert("associated".into(), Json::Object(bindings));
            }
            Json::Object(interface)
        }
        TyTemplate::Enum(head) => {
            let mut enm = named("enum", head);
            reference(&mut enm, "def", "definition", head.tag(), defs);
            Json::Object(enm)
        }
        TyTemplate::EnumVariant(head, variant) => {
            let mut variant_ty = named("enumVariant", head);
            variant_ty.insert("value".into(), Json::from(variant.as_str()));
            reference(&mut variant_ty, "def", "definition", head.tag(), defs);
            Json::Object(variant_ty)
        }
        TyTemplate::TypeAlias(head) => Json::Object(named("alias", head)),
        TyTemplate::List(item) => node("list", [("item", template(item, defs))]),
        TyTemplate::Map { key, value } => {
            let key = template(key, defs);
            node("map", [("key", key), ("value", template(value, defs))])
        }
        TyTemplate::Union(members) => {
            let non_null: Vec<_> = members
                .iter()
                .filter(|member| !matches!(member, TyTemplate::Null))
                .collect();
            if non_null.len() == members.len() {
                return node("union", [("variants", list(members, defs))]);
            }
            match non_null.as_slice() {
                [] => leaf("null"),
                [only] => node("optional", [("inner", template(only, defs))]),
                many => {
                    let variants = many.iter().map(|member| template(member, defs)).collect();
                    node(
                        "optional",
                        [(
                            "inner",
                            node("union", [("variants", Json::Array(variants))]),
                        )],
                    )
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
                    part.insert("schema".into(), template(&param.ty, defs));
                    if param.is_optional() {
                        part.insert("optional".into(), Json::Bool(true));
                    }
                    Json::Object(part)
                })
                .collect();
            let ret = template(ret, defs);
            node(
                "function",
                [
                    ("params", Json::Array(params)),
                    ("return", ret),
                    ("throws", template(throws, defs)),
                ],
            )
        }
        TyTemplate::Future(value, error) => {
            let value = template(value, defs);
            node(
                "future",
                [("value", value), ("error", template(error, defs))],
            )
        }
        TyTemplate::RustType => leaf("rustType"),
        TyTemplate::Type => leaf("type"),
        TyTemplate::Resource => leaf("resource"),
        TyTemplate::PromptAst => leaf("promptAst"),
        TyTemplate::Void => leaf("void"),
        TyTemplate::Unknown => leaf("unknown"),
        TyTemplate::Never => leaf("never"),
        TyTemplate::TypeArgRef(index) => node("typeParam", [("index", Json::from(*index))]),
        TyTemplate::AssociatedTypeProjection { member, .. } => {
            node("projection", [("member", Json::from(member.as_str()))])
        }
    }
}

/// Add `tag`'s definition id under `id_key`, and on its first reference in
/// the cell the definition under `definition_key`.
pub(super) fn reference(
    map: &mut Map<String, Json>,
    id_key: &str,
    definition_key: &str,
    tag: Option<TypeTag>,
    defs: &mut dyn CellDefinitions,
) {
    let Some((id, declaration)) = tag.and_then(|tag| defs.reference(tag)) else {
        return;
    };
    map.insert(id_key.into(), Json::from(id.as_ref()));
    if let Some(declaration) = declaration {
        map.insert(definition_key.into(), definition(&declaration, defs));
    }
}

/// A definition body; keys that are absent, empty or false are omitted.
fn definition(d: &TypeDeclaration, defs: &mut dyn CellDefinitions) -> Json {
    let mut map = Map::new();
    map.insert(
        "kind".into(),
        Json::from(if d.is_enum { "enum" } else { "class" }),
    );
    map.insert("name".into(), Json::from(d.name.to_string()));
    if d.type_params > 0 {
        map.insert("type_params".into(), Json::from(d.type_params));
    }
    metadata(
        &mut map,
        d.description.as_deref(),
        d.alias.as_deref(),
        d.docstring.as_deref(),
        &d.attributes,
    );
    flag(&mut map, "stream_done", d.stream_done);
    if d.is_enum {
        let variants = d
            .variants
            .iter()
            .map(|v| {
                let mut variant = Map::new();
                variant.insert("name".into(), Json::from(v.name.as_str()));
                metadata(
                    &mut variant,
                    v.description.as_deref(),
                    v.alias.as_deref(),
                    v.docstring.as_deref(),
                    &v.attributes,
                );
                flag(&mut variant, "skip", v.skip);
                Json::Object(variant)
            })
            .collect();
        map.insert("variants".into(), Json::Array(variants));
    } else {
        let fields = d
            .fields
            .iter()
            .map(|f| {
                let mut field = Map::new();
                field.insert("name".into(), Json::from(f.name.as_str()));
                field.insert("schema".into(), template(&f.schema, defs));
                metadata(
                    &mut field,
                    f.description.as_deref(),
                    f.alias.as_deref(),
                    f.docstring.as_deref(),
                    &f.attributes,
                );
                flag(&mut field, "skip", f.skip);
                flag(&mut field, "stream_done", f.stream_done);
                flag(&mut field, "must_exist", f.must_exist);
                Json::Object(field)
            })
            .collect();
        map.insert("fields".into(), Json::Array(fields));
    }
    Json::Object(map)
}

fn metadata(
    map: &mut Map<String, Json>,
    description: Option<&str>,
    alias: Option<&str>,
    docstring: Option<&str>,
    attributes: &[(String, String)],
) {
    for (key, value) in [
        ("description", description),
        ("alias", alias),
        ("docstring", docstring),
    ] {
        if let Some(value) = value {
            map.insert(key.into(), Json::from(value));
        }
    }
    if !attributes.is_empty() {
        let attributes = attributes
            .iter()
            .map(|(key, value)| (key.clone(), Json::from(value.as_str())))
            .collect();
        map.insert("attributes".into(), Json::Object(attributes));
    }
}

fn flag(map: &mut Map<String, Json>, key: &str, set: bool) {
    if set {
        map.insert(key.into(), Json::Bool(true));
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

fn named(name: &str, head: &impl Head) -> Map<String, Json> {
    let mut map = kind(name);
    map.insert("name".into(), head.spelling());
    map
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
