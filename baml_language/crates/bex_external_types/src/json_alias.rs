//! The recursive `baml.json.json` alias at the host boundary: which name is
//! the alias, and which values are in its algebra.

use baml_type::{Literal, RuntimeTy, TypeName};

use crate::BexExternalValue;

/// Canonical dotted path of the builtin JSON alias — test fixtures build the
/// builtin `TypeName` from it; runtime identity checks go through
/// [`is_canonical_json_alias`], never a rendered string.
#[cfg(test)]
const BAML_JSON_JSON: &str = "baml.json.json";

/// Whether a type name is the canonical builtin `baml.json.json` alias.
///
/// Compares the fully qualified identity — package `baml`, namespace
/// `json`, name `json` — never a rendered display string:
/// `TypeName::display_name()` elides the implicit `user` package for local
/// types, so a user alias declared at namespace path `baml.json` with name
/// `json` would render identically and must NOT receive builtin JSON
/// behavior.
pub fn is_canonical_json_alias(name: &TypeName) -> bool {
    name.package().as_str() == "baml"
        && name.namespace().len() == 1
        && name.namespace()[0].as_str() == "json"
        && name.name().as_str() == "json"
}

/// Whether an external value is exactly in the recursive `baml.json.json`
/// algebra. BAML extensions such as bytes, classes, enums, media, handles, and
/// non-finite floats are intentionally rejected.
///
/// A sparse inbound `value_type` annotation (a transient
/// `BexExternalValue::Union` with `is_inbound_type_annotation`, e.g. the
/// Swift bridge annotates every json scalar leaf) is peeled — but only when
/// the annotation itself stays within the JSON algebra, so a payload
/// annotated as a class is still rejected. A genuine union carrier (a value
/// produced from a declared union) is never JSON.
pub fn value_satisfies_json(value: &BexExternalValue) -> bool {
    fn recurse(value: &BexExternalValue, depth: usize) -> bool {
        if depth > 256 {
            return false;
        }
        match value {
            BexExternalValue::Null
            | BexExternalValue::Int(_)
            | BexExternalValue::Bigint(_)
            | BexExternalValue::Bool(_)
            | BexExternalValue::String(_) => true,
            BexExternalValue::Float(value) => value.is_finite(),
            BexExternalValue::Array { items, .. } => {
                items.iter().all(|item| recurse(item, depth + 1))
            }
            BexExternalValue::Map { entries, .. } => {
                entries.values().all(|item| recurse(item, depth + 1))
            }
            BexExternalValue::Union { value, metadata } => {
                metadata.is_inbound_type_annotation
                    && runtime_ty_within_json_algebra(&metadata.selected_option, 0)
                    && recurse(value, depth + 1)
            }
            _ => false,
        }
    }

    recurse(value, 0)
}

/// Whether a declared or annotated `RuntimeTy` lies entirely within the
/// recursive `baml.json.json` algebra: the alias itself, the JSON scalar
/// primitives, their literals, and string-keyed containers thereof.
fn runtime_ty_within_json_algebra(ty: &RuntimeTy, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    match ty {
        RuntimeTy::Null
        | RuntimeTy::Bool
        | RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String => true,
        RuntimeTy::TypeAlias(name) => is_canonical_json_alias(name),
        RuntimeTy::Literal(literal, _) => matches!(
            literal,
            Literal::Bool(_)
                | Literal::Int(_)
                | Literal::Bigint(_)
                | Literal::String(_)
                | Literal::Float(_)
        ),
        RuntimeTy::List(inner) => runtime_ty_within_json_algebra(inner, depth + 1),
        RuntimeTy::Map { key, value, .. } => {
            matches!(key.as_ref(), RuntimeTy::String)
                && runtime_ty_within_json_algebra(value, depth + 1)
        }
        RuntimeTy::Union(members) => {
            !members.is_empty()
                && members
                    .iter()
                    .all(|member| runtime_ty_within_json_algebra(member, depth + 1))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{RuntimeTy, TypeName};
    use indexmap::IndexMap;

    use super::*;
    use crate::BexExternalValue;

    fn json_ty() -> RuntimeTy {
        RuntimeTy::TypeAlias(TypeName::from_dotted_path(BAML_JSON_JSON))
    }

    #[test]
    fn canonical_json_alias_accepts_only_the_json_value_algebra() {
        let mut nested = IndexMap::new();
        nested.insert(
            "items".to_string(),
            BexExternalValue::Array {
                element_type: RuntimeTy::unknown(),
                items: vec![
                    BexExternalValue::Null,
                    BexExternalValue::Bool(true),
                    BexExternalValue::Int(7),
                    BexExternalValue::Bigint(123123123123123123123123_u128.into()),
                    BexExternalValue::Float(1.5),
                    BexExternalValue::String("ok".into()),
                ],
            },
        );
        let valid = BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries: nested,
        };
        assert!(value_satisfies_json(&valid));
        assert!(!value_satisfies_json(&BexExternalValue::Float(f64::NAN)));
        assert!(value_satisfies_json(&BexExternalValue::Bigint(1.into())));
        assert!(!value_satisfies_json(&BexExternalValue::Uint8Array(vec![
            1
        ])));
        assert!(!value_satisfies_json(&BexExternalValue::Instance {
            class_name: "JsonLooking".to_string(),
            type_args: vec![],
            fields: IndexMap::new(),
        }));

        let forged = BexExternalValue::union(
            BexExternalValue::String("json-shaped payload".into()),
            [RuntimeTy::bigint(), RuntimeTy::string()],
            RuntimeTy::bigint(),
        );
        assert!(!value_satisfies_json(&forged));
    }

    #[test]
    fn user_alias_shadowing_json_display_name_is_not_canonical() {
        // `display_name()` elides the implicit `user` package, so a user alias
        // declared at namespace path `baml.json` with name `json` renders
        // identically to the builtin. Identity must be package-qualified.
        let builtin = TypeName::from_dotted_path(BAML_JSON_JSON);
        let shadow = TypeName::from_dotted_path("user.baml.json.json");
        assert_eq!(
            builtin.display_name().as_str(),
            shadow.display_name().as_str()
        );
        assert!(is_canonical_json_alias(&builtin));
        assert!(!is_canonical_json_alias(&shadow));
    }

    #[test]
    fn canonical_json_alias_accepts_algebra_scoped_inbound_annotations() {
        // The C++ codec annotates a json return's selected variant alternative
        // (`map<string, baml.json.json>`); Swift annotates scalar leaves. Both
        // are sparse inbound annotations inside the JSON algebra and must
        // validate against a declared json return — including nested in a
        // container — while annotations outside the algebra stay rejected.
        let mut entries = IndexMap::new();
        entries.insert(
            "type".to_string(),
            BexExternalValue::typed(BexExternalValue::String("ok".into()), RuntimeTy::string()),
        );
        let annotated_map = BexExternalValue::typed(
            BexExternalValue::Map {
                key_type: RuntimeTy::string(),
                value_type: RuntimeTy::unknown(),
                entries,
            },
            RuntimeTy::Map {
                key: Box::new(RuntimeTy::string()),
                value: Box::new(json_ty()),
            },
        );
        assert!(value_satisfies_json(&annotated_map));

        let annotated_bigint =
            BexExternalValue::typed(BexExternalValue::Bigint(1.into()), RuntimeTy::bigint());
        assert!(value_satisfies_json(&annotated_bigint));
    }
}
