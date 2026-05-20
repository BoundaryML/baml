// Output formatting for `BexExternalValue` — shared by `baml run` and
// packaged binaries produced by `baml pack`.
//
// Two formats, per BEP-027:
//   - Debug: human-readable, with type annotations. Default for `baml run`.
//   - Json:  single JSON document, no wrapping. Default for `baml pack`,
//            designed for pipelines / CI / agents.

#![allow(clippy::print_stdout)]

use std::sync::Arc;

use anyhow::{Result, anyhow};
use bex_engine::{BexEngine, BexExternalValue, CallId, FunctionCallContextBuilder, Ty};

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
/// `json` mode dispatches to `baml.json.to_string<T>` so the spec's
/// "no wrapping" rule is enforced by the stdlib serializer (which also
/// honors user `to_json` overrides). `debug` mode uses [`format_value`].
pub async fn write_output(
    engine: &Arc<BexEngine>,
    value: BexExternalValue,
    return_type: &Ty,
    format: OutputFormat,
) -> Result<()> {
    match format {
        OutputFormat::Debug => {
            // TODO: route debug mode through a stdlib `baml.debug.format<T>`
            // (or auto-derived `to_string` on user classes) so user-defined
            // display overrides are honored, mirroring how `json` mode goes
            // through `baml.json.to_string<T>`. The stdlib doesn't expose a
            // general value-to-debug-string today — adding it is a separate
            // BEP. Until then, the structural pretty-printer below is what
            // the spec calls "human-readable with type annotations".
            println!("{}", format_value(&value));
            Ok(())
        }
        OutputFormat::Json => {
            let text = serialize_via_baml_json(engine, value, return_type).await?;
            println!("{text}");
            Ok(())
        }
    }
}

/// Serialize a value via the BAML stdlib's `baml.json.serialize<T>`.
///
/// `serialize<T>` composes `stringify(to_json<T>(v))` in BAML so the
/// dynamic-dispatch path through `<Class>.to_json()` honors user
/// overrides. The alternative, `baml.json.to_string<T>`, is the Rust
/// structural walker and bypasses overrides — appropriate when override
/// behavior would corrupt downstream consumers, but the wrong default
/// for `baml run` / `baml pack` output.
async fn serialize_via_baml_json(
    engine: &Arc<BexEngine>,
    value: BexExternalValue,
    return_type: &Ty,
) -> Result<String> {
    let result = engine
        .call_function(
            "baml.json.serialize",
            vec![value],
            FunctionCallContextBuilder::new(CallId::next())
                .with_type_args(vec![return_type.clone()])
                .build(),
            true,
        )
        .await
        .map_err(|e| anyhow!("baml.json.serialize failed: {e:?}"))?;
    match result {
        BexExternalValue::String(s) => Ok(s),
        other => Err(anyhow!(
            "baml.json.serialize returned non-string value: {other:?}"
        )),
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
