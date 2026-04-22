// Output formatting for `BexExternalValue` — shared by `baml run` and
// packaged binaries produced by `baml pack`.
//
// Two formats, per BEP-027:
//   - Debug: human-readable, with type annotations. Default for `baml run`.
//   - Json:  single JSON document, no wrapping. Default for `baml pack`,
//            designed for pipelines / CI / agents.

#![allow(clippy::print_stdout)]

use bex_engine::{BexExternalValue, Ty};

/// Serialization format for a target's return value.
#[derive(Copy, Clone, Debug, Default, serde::Serialize, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable formatting with type annotations.
    #[default]
    Debug,
    /// Single JSON document with no wrapping noise.
    Json,
}

/// Write the target's return value to stdout per the selected format.
///
/// A `null` return produces nothing; the caller is expected to skip
/// printing for "no return value" targets.
pub fn write_output(value: &BexExternalValue, format: OutputFormat) {
    match format {
        OutputFormat::Debug => println!("{}", format_value(value)),
        OutputFormat::Json => {
            let json = external_to_json(value);
            println!(
                "{}",
                serde_json::to_string_pretty(&json).unwrap_or_else(|_| "null".to_string())
            );
        }
    }
}

/// Convert a `BexExternalValue` to a `serde_json::Value` for JSON output.
pub fn external_to_json(value: &BexExternalValue) -> serde_json::Value {
    match value {
        BexExternalValue::Null => serde_json::Value::Null,
        BexExternalValue::Int(i) => serde_json::json!(i),
        BexExternalValue::Float(f) => serde_json::json!(f),
        BexExternalValue::Bool(b) => serde_json::json!(b),
        BexExternalValue::String(s) => serde_json::json!(s),
        BexExternalValue::Array { items, .. } => {
            serde_json::Value::Array(items.iter().map(external_to_json).collect())
        }
        BexExternalValue::Map { entries, .. } => serde_json::Value::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), external_to_json(v)))
                .collect(),
        ),
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let mut map: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|(k, v)| (k.clone(), external_to_json(v)))
                .collect();
            if !class_name.is_empty() {
                map.insert("__type".to_string(), serde_json::json!(class_name));
            }
            serde_json::Value::Object(map)
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => serde_json::json!({ "__type": enum_name, "value": variant_name }),
        BexExternalValue::Union { value, .. } => external_to_json(value),
        BexExternalValue::Uint8Array(bytes) => {
            serde_json::json!(format!("<bytes:{}>", bytes.len()))
        }
        _ => serde_json::json!(format!("{value:?}")),
    }
}

/// Human-readable formatting for `BexExternalValue`.
pub fn format_value(value: &BexExternalValue) -> String {
    match value {
        BexExternalValue::Null => "null".to_string(),
        BexExternalValue::Int(i) => i.to_string(),
        BexExternalValue::Float(f) => {
            let s = f.to_string();
            if s.contains('.') || !f.is_finite() {
                s
            } else {
                format!("{s}.0")
            }
        }
        BexExternalValue::Bool(b) => b.to_string(),
        BexExternalValue::String(s) => format!("{s:?}"),
        BexExternalValue::Array { items, .. } => {
            let inner: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", inner.join(", "))
        }
        BexExternalValue::Map { entries, .. } => {
            let inner: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{k:?}: {}", format_value(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        BexExternalValue::Instance { class_name, fields } => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_value(v)))
                .collect();
            if class_name.is_empty() {
                format!("{{{}}}", inner.join(", "))
            } else {
                format!("{class_name} {{{}}}", inner.join(", "))
            }
        }
        BexExternalValue::Variant { variant_name, .. } => variant_name.clone(),
        BexExternalValue::Union { value, .. } => format_value(value),
        BexExternalValue::Uint8Array(bytes) => format!("<bytes:{}>", bytes.len()),
        _ => format!("{value:?}"),
    }
}

/// Generate a placeholder example value for a type (used in `--help` output).
pub fn example_value(ty: &Ty) -> &'static str {
    match ty {
        Ty::String { .. } => "\"value\"",
        Ty::Int { .. } => "42",
        Ty::Float { .. } => "3.14",
        Ty::Bool { .. } => "true",
        Ty::Null { .. } => "null",
        Ty::Enum(..) => "VariantName",
        _ => "...",
    }
}
