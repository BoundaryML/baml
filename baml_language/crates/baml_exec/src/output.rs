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
use bex_engine::{BexEngine, BexExternalValue, RuntimeTy};

use crate::HelperCallContext;

/// Serialization format for a target's return value.
#[derive(
    Copy, Clone, Debug, Default, borsh::BorshSerialize, borsh::BorshDeserialize, clap::ValueEnum,
)]
pub enum OutputFormat {
    /// Human-readable formatting with type annotations.
    #[default]
    Debug,
    /// Single JSON document with no wrapping noise.
    Json,
}

/// Write the target's return value to stdout per the selected format.
///
/// `json` mode dispatches to the stdlib JSON serializer so the spec's
/// "no wrapping" rule is enforced by the stdlib serializer (which also
/// honors user `to_json` overrides). `debug` mode uses [`format_value`].
pub async fn write_output(
    engine: &Arc<BexEngine>,
    value: BexExternalValue,
    return_type: &RuntimeTy,
    format: OutputFormat,
) -> Result<()> {
    write_output_with_context(
        engine,
        value,
        return_type,
        format,
        &HelperCallContext::disabled(),
        || {},
    )
    .await
}

/// Write a target's return value while preserving logger/cancellation context
/// for conversion hooks.
///
/// `before_print` runs after any user-defined `to_json` hook and before the
/// serialized value is written, allowing callers to keep captured logs ordered
/// ahead of the target result.
pub async fn write_output_with_context(
    engine: &Arc<BexEngine>,
    value: BexExternalValue,
    return_type: &RuntimeTy,
    format: OutputFormat,
    helper_context: &HelperCallContext,
    before_print: impl FnOnce(),
) -> Result<()> {
    match format {
        OutputFormat::Debug => {
            // TODO: route debug mode through a stdlib `baml.debug.format<T>`
            // (or auto-derived `to_string` on user classes) so user-defined
            // display overrides are honored, mirroring how `json` mode goes
            // through the stdlib JSON serializer. The stdlib doesn't expose a
            // general value-to-debug-string today — adding it is a separate
            // BEP. Until then, the structural pretty-printer below is what
            // the spec calls "human-readable with type annotations".
            before_print();
            println!("{}", format_value(&value));
            Ok(())
        }
        OutputFormat::Json => {
            let text = serialize_via_baml_json(engine, value, return_type, helper_context).await?;
            before_print();
            println!("{text}");
            Ok(())
        }
    }
}

/// Serialize a value via the BAML stdlib's `baml.json.serialize<T>`.
///
/// `serialize<T>` delegates to `baml.json.to_string(v)` in BAML, whose
/// runtime-value dispatch honors user `baml.ToJson` overrides at every depth.
async fn serialize_via_baml_json(
    engine: &Arc<BexEngine>,
    value: BexExternalValue,
    return_type: &RuntimeTy,
    helper_context: &HelperCallContext,
) -> Result<String> {
    let result = engine
        .call_function(
            "baml.json.serialize",
            vec![value],
            helper_context.call_context(indexmap::IndexMap::from([(
                "T".to_string(),
                return_type.clone(),
            )])),
            true,
        )
        .await
        .map_err(|e| anyhow!("baml.json.serialize failed: {e:?}"))?;
    match result {
        BexExternalValue::String(s) => Ok(s.to_string()),
        other => Err(anyhow!(
            "baml.json.serialize returned non-string value: {other:?}"
        )),
    }
}

/// Human-readable formatting for `BexExternalValue`.
///
/// Thin wrapper over [`BexExternalValue::render_readable`] — the canonical
/// structural renderer, shared with the engine's uncaught-throw rendering so
/// `baml run` output and a leaked `throw` render identically.
pub fn format_value(value: &BexExternalValue) -> String {
    value.render_readable()
}
