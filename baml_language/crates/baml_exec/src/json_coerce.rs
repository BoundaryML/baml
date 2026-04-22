// JSON → `BexExternalValue` coercion, type-driven where a target `Ty` is
// available and best-effort otherwise.
//
// `json_to_external_with_ty` is the preferred entrypoint: it makes enum JSON
// become `Variant`, object JSON become `Instance { class_name }` with the
// correct name, and lists/maps carry the declared element/value types.
// `json_to_external` is the untyped fallback for unknown `--json-args` keys
// and nested class fields whose schema isn't available at this layer.

use anyhow::{Context, Result, anyhow};
use baml_type::TyAttr;
use bex_engine::{BexExternalValue, Ty};

/// Load JSON from the `--json-args` source: inline string, `@file`, or `-` for stdin.
pub fn load_json_source(source: &str) -> Result<serde_json::Value> {
    if source == "-" {
        let input =
            std::io::read_to_string(std::io::stdin()).context("Failed to read JSON from stdin")?;
        serde_json::from_str(&input).context("Invalid JSON from stdin")
    } else if let Some(path) = source.strip_prefix('@') {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {path}"))?;
        serde_json::from_str(&content).with_context(|| format!("Invalid JSON in file: {path}"))
    } else {
        serde_json::from_str(source).context("Invalid inline JSON for --json-args")
    }
}

/// Recursively convert a `serde_json::Value` to `BexExternalValue` with no
/// type information. Used as a fallback when the target type is unknown.
pub fn json_to_external(value: &serde_json::Value) -> BexExternalValue {
    match value {
        serde_json::Value::Null => BexExternalValue::Null,
        serde_json::Value::Bool(b) => BexExternalValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                BexExternalValue::Int(i)
            } else {
                BexExternalValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => BexExternalValue::String(s.clone()),
        serde_json::Value::Array(items) => BexExternalValue::Array {
            element_type: Ty::String {
                attr: TyAttr::default(),
            },
            items: items.iter().map(json_to_external).collect(),
        },
        serde_json::Value::Object(map) => BexExternalValue::Instance {
            class_name: String::new(),
            fields: map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_external(v)))
                .collect(),
        },
    }
}

/// Convert a `serde_json::Value` to a `BexExternalValue` using the target
/// `Ty` to drive coercion. Class field types aren't resolved here (we don't
/// have the class schema at this layer), so nested class fields fall back
/// to [`json_to_external`].
pub fn json_to_external_with_ty(value: &serde_json::Value, ty: &Ty) -> Result<BexExternalValue> {
    use serde_json::Value as J;
    match ty {
        Ty::Optional(inner, _) => {
            if matches!(value, J::Null) {
                Ok(BexExternalValue::Null)
            } else {
                json_to_external_with_ty(value, inner)
            }
        }

        Ty::Null { .. } => match value {
            J::Null => Ok(BexExternalValue::Null),
            _ => anyhow::bail!("Expected null, got `{value}`"),
        },

        Ty::Bool { .. } => match value {
            J::Bool(b) => Ok(BexExternalValue::Bool(*b)),
            _ => anyhow::bail!("Expected bool, got `{value}`"),
        },

        Ty::Int { .. } => match value {
            J::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(BexExternalValue::Int(i))
                } else if let Some(u) = n.as_u64() {
                    i64::try_from(u)
                        .map(BexExternalValue::Int)
                        .map_err(|_| anyhow!("Integer out of range for int: {u}"))
                } else {
                    anyhow::bail!("Expected integer, got `{value}`")
                }
            }
            _ => anyhow::bail!("Expected integer, got `{value}`"),
        },

        Ty::Float { .. } => match value {
            J::Number(n) => n
                .as_f64()
                .map(BexExternalValue::Float)
                .ok_or_else(|| anyhow!("Expected float, got `{value}`")),
            _ => anyhow::bail!("Expected float, got `{value}`"),
        },

        Ty::String { .. } => match value {
            J::String(s) => Ok(BexExternalValue::String(s.clone())),
            _ => anyhow::bail!("Expected string, got `{value}`"),
        },

        Ty::Enum(type_name, _) => match value {
            J::String(s) => Ok(BexExternalValue::Variant {
                enum_name: type_name.display_name.to_string(),
                variant_name: s.clone(),
            }),
            _ => anyhow::bail!(
                "Expected enum variant name (string) for `{}`, got `{value}`",
                type_name.display_name
            ),
        },

        Ty::Class(type_name, _) => match value {
            J::Object(map) => Ok(BexExternalValue::Instance {
                class_name: type_name.display_name.to_string(),
                fields: map
                    .iter()
                    .map(|(k, v)| (k.clone(), json_to_external(v)))
                    .collect(),
            }),
            _ => anyhow::bail!(
                "Expected object for class `{}`, got `{value}`",
                type_name.display_name
            ),
        },

        Ty::List(inner, _) => match value {
            J::Array(items) => {
                let mut converted = Vec::with_capacity(items.len());
                for item in items {
                    converted.push(json_to_external_with_ty(item, inner)?);
                }
                Ok(BexExternalValue::Array {
                    element_type: (**inner).clone(),
                    items: converted,
                })
            }
            _ => anyhow::bail!("Expected array for `{ty}`, got `{value}`"),
        },

        Ty::Map {
            key,
            value: value_ty,
            ..
        } => match value {
            J::Object(map) => {
                let mut pairs = Vec::with_capacity(map.len());
                for (k, v) in map {
                    pairs.push((k.clone(), json_to_external_with_ty(v, value_ty)?));
                }
                Ok(BexExternalValue::Map {
                    key_type: (**key).clone(),
                    value_type: (**value_ty).clone(),
                    entries: pairs.into_iter().collect(),
                })
            }
            _ => anyhow::bail!("Expected object for map `{ty}`, got `{value}`"),
        },

        Ty::Union(variants, _) => coerce_json_union(value, variants),

        _ => Ok(json_to_external(value)),
    }
}

/// Best-effort coercion into a union: try each variant and return the first
/// that succeeds. On failure, surface the last variant's error.
fn coerce_json_union(value: &serde_json::Value, variants: &[Ty]) -> Result<BexExternalValue> {
    let mut last_err: Option<anyhow::Error> = None;
    for variant in variants {
        match json_to_external_with_ty(value, variant) {
            Ok(v) => return Ok(v),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("No union variant matched value `{value}`")))
}
