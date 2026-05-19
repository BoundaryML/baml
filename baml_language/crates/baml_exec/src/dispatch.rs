// `dispatch_target` is the shared entrypoint for both `baml run` and the
// `baml-pack-host` runtime that packaged binaries ship with.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, CallId, EngineError, FunctionCallContextBuilder, Ty,
};

use crate::{
    auto_cli::parse_auto_cli_args,
    output::{OutputFormat, write_output},
};

/// Result of dispatching a target.
pub enum DispatchResult {
    /// Target completed successfully.
    Ok,
    /// Target raised an error (already printed to stderr).
    TargetError,
    /// Target called `baml.sys.exit(code)`. The caller is responsible for
    /// terminating the process with this code (clamped to the shell's
    /// range as appropriate — typically 0..=255 on Unix).
    Exit(i64),
}

/// Reject targets whose signature declares a parameter named `help`.
pub fn validate_help_param(engine: &BexEngine, function_name: &str) -> Result<()> {
    let params = engine
        .function_params(function_name)
        .with_context(|| format!("Failed to resolve target `{function_name}`"))?;
    if params.iter().any(|(name, _, _)| *name == "help") {
        anyhow::bail!(
            "Target `{function_name}` declares a parameter named `help`, \
             which collides with the auto-derived `--help` flag. \
             Rename this parameter to be used as an entry point."
        );
    }
    Ok(())
}

/// Narrow a `baml.sys.exit(code)` value (BAML `int` = `i64`) to the `i32`
/// that `std::process::exit` and C's `exit(int)` take.
pub fn clamp_exit_code(code: i64) -> i32 {
    i32::try_from(code).unwrap_or(if code < 0 { i32::MIN } else { i32::MAX })
}

/// Invoke `target_name` with parameters drawn from `cli_tokens` (and
/// optionally `json_args`), then write the return value to stdout.
pub async fn dispatch_target(
    engine: Arc<BexEngine>,
    target_name: &str,
    cli_tokens: &[String],
    json_args: Option<serde_json::Value>,
    output_format: OutputFormat,
) -> Result<DispatchResult> {
    let func_info = engine
        .find_user_function(target_name)
        .ok_or_else(|| anyhow!("Function `{target_name}` not found"))?;

    // BEP-027 §"Auto-CLI conventions": `help` is reserved at entry-point
    // resolution under both `baml run` and `baml pack`. Pack catches this
    // at pack time; checking again here covers the run side and is a
    // belt-and-suspenders against future host callers. Pass the canonical
    // post-resolved name so the validator sees the same identifier that
    // `find_user_function` matched, not the raw user input.
    validate_help_param(&engine, &func_info.qualified_name)?;

    let args = build_args_from_signature(
        &engine,
        cli_tokens,
        json_args.as_ref(),
        &func_info.param_names,
        &func_info.param_types,
        &func_info.param_has_default,
    )
    .await?;

    let result = engine
        .call_function_bound_args(
            target_name,
            args,
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(value) => {
            // No stdout for `void` return; value-carrying types like `int?`
            // still emit their serialization even when null.
            if !matches!(func_info.return_type, Ty::Void { .. }) {
                write_output(&engine, value, &func_info.return_type, output_format).await?;
            }
            Ok(DispatchResult::Ok)
        }
        Err(EngineError::Exit { code }) => Ok(DispatchResult::Exit(code)),
        Err(e) => {
            eprintln!("Error: {e:#}");
            Ok(DispatchResult::TargetError)
        }
    }
}

/// Raw form of a parameter input before engine-side coercion.
///
/// `BexExternalValue` is ~344 bytes (it holds an inline `String` /
/// nested array), so the variant is boxed — without boxing every
/// `JsonText` slot in the merged map would carry that footprint
/// regardless of which branch it belongs to (`clippy::large_enum_variant`).
enum RawArg {
    /// Already-typed primitive from auto-CLI (string/int/float/bool/null/enum).
    Primitive(Box<BexExternalValue>),
    /// JSON text from `--json-args` (the only path that delivers
    /// structured values per BEP-027 §"Open questions" #5). Resolved
    /// asynchronously via `baml.json.deserialize<T>`.
    JsonText(String),
}

/// Build the ordered argument vector for a call by merging `--json-args`
/// and auto-CLI flags. CLI flags override JSON keys (BEP-027 §"JSON
/// argument form").
///
/// Complex-typed parameters are coerced through `baml.json.deserialize<T>`
/// so user `from_json` overrides on classes are honored — same model as
/// the output side using `baml.json.serialize<T>`. Primitive parameters
/// skip the engine and bind directly.
pub async fn build_args_from_signature(
    engine: &Arc<BexEngine>,
    cli_tokens: &[String],
    json_args: Option<&serde_json::Value>,
    param_names: &[String],
    param_types: &[Ty],
    param_has_default: &[bool],
) -> Result<Vec<BexCallArg>> {
    let mut merged: HashMap<String, RawArg> = HashMap::new();

    // `--json-args` first (lower priority — CLI overrides).
    if let Some(json) = json_args {
        let obj = json
            .as_object()
            .ok_or_else(|| anyhow!("--json-args must be a JSON object, got: {json}"))?;
        for (key, value) in obj {
            // Re-stringify each field so the engine deserializer takes a
            // fresh JSON document. Round-tripping via `to_string` is
            // cheaper than implementing a parallel json-value-to-engine
            // path; per-arg JSON is typically tiny.
            merged.insert(key.clone(), RawArg::JsonText(value.to_string()));
        }
    }

    // Auto-CLI flags override. After the spec-strict refactor, auto-CLI
    // only produces typed primitives (string/int/float/bool/null/enum) —
    // structured types must come through `--json-args`.
    let cli_map = parse_auto_cli_args(cli_tokens, param_names, param_types, param_has_default)?;
    for (key, value) in cli_map {
        merged.insert(key, RawArg::Primitive(Box::new(value)));
    }

    let mut ordered = Vec::with_capacity(param_names.len());
    for (i, name) in param_names.iter().enumerate() {
        let ty = &param_types[i];
        let has_default = param_has_default.get(i).copied().unwrap_or(false);
        match merged.remove(name.as_str()) {
            Some(RawArg::Primitive(v)) => ordered.push(BexCallArg::Provided(Box::new(*v))),
            Some(RawArg::JsonText(s)) => {
                let value = deserialize_via_baml_json(engine, &s, ty)
                    .await
                    .with_context(|| format!("parameter `--{name}`"))?;
                ordered.push(BexCallArg::Provided(Box::new(value)));
            }
            None if has_default => ordered.push(BexCallArg::OmittedDefault),
            None => {
                // BEP-027 §"Auto-CLI conventions": flags live after `--`
                // under `baml run`. Spell that out so first-time users
                // don't trip on the separator. Packaged binaries have no
                // `--`, so the suggestion's `--name <value>` still
                // applies as-is when the host is a packed binary.
                anyhow::bail!(
                    "Missing required argument `--{name}` (type: {ty}).\n\
                     Pass it after `--`: ... -- --{name} <value>"
                );
            }
        }
    }

    if !merged.is_empty() {
        let unknown: Vec<&str> = merged.keys().map(String::as_str).collect();
        eprintln!(
            "Warning: unknown argument(s) ignored: {}",
            unknown.join(", ")
        );
    }

    Ok(ordered)
}

/// Coerce JSON text into a typed BAML value via the stdlib's
/// `baml.json.deserialize<T>` (which dispatches user `from_json` overrides).
async fn deserialize_via_baml_json(
    engine: &Arc<BexEngine>,
    json_text: &str,
    ty: &Ty,
) -> Result<BexExternalValue> {
    let result = engine
        .call_function(
            "baml.json.deserialize",
            vec![BexExternalValue::String(json_text.to_string())],
            FunctionCallContextBuilder::new(CallId::next())
                .with_type_args(vec![ty.clone()])
                .build(),
            true,
        )
        .await
        .map_err(|e| anyhow!("baml.json.deserialize failed: {e:?}"))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use baml_type::TyAttr;
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    use super::*;

    fn engine(source: &str) -> Arc<BexEngine> {
        let snapshot = baml_tests::engine::compile_source(source);
        Arc::new(
            BexEngine::new(
                snapshot,
                Arc::new(sys_native::SysOps::native()),
                None,
                Vec::new(),
            )
            .expect("BexEngine::new should succeed"),
        )
    }

    fn ty_string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }

    // ── clamp_exit_code ─────────────────────────────────────────────────

    #[test]
    fn clamp_exit_code_in_range_is_lossless() {
        assert_eq!(clamp_exit_code(0), 0);
        assert_eq!(clamp_exit_code(1), 1);
        assert_eq!(clamp_exit_code(7), 7);
        assert_eq!(clamp_exit_code(255), 255);
    }

    /// BAML `int` = `i64`; the C `exit(int)` boundary saturates rather
    /// than wrapping so a user-supplied `2_000_000_000_000` doesn't roll
    /// over to a tiny non-zero exit code by accident.
    #[test]
    fn clamp_exit_code_saturates_on_overflow() {
        assert_eq!(clamp_exit_code(i64::MAX), i32::MAX);
        assert_eq!(clamp_exit_code(i64::MIN), i32::MIN);
    }

    // ── validate_help_param ─────────────────────────────────────────────

    /// BEP-027 §"Auto-CLI conventions": *"A typed target whose signature
    /// declares a parameter named `help` cannot be used as an entry point."*
    #[tokio::test]
    async fn validate_help_param_rejects_help_named_param() {
        let eng = engine(r#"function Entry(help: string) -> string { help }"#);
        let err = validate_help_param(&eng, "user.Entry").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`help`"), "got: {msg}");
        assert!(msg.to_lowercase().contains("rename"), "got: {msg}");
    }

    #[tokio::test]
    async fn validate_help_param_allows_other_names() {
        let eng = engine(r#"function Entry(text: string) -> string { text }"#);
        validate_help_param(&eng, "user.Entry").unwrap();
    }

    #[tokio::test]
    async fn validate_help_param_parameterless_passes() {
        let eng = engine("function main() -> int { 1 }");
        validate_help_param(&eng, "user.main").unwrap();
    }

    /// An unresolvable name must surface as an error rather than silently
    /// passing validation. The previous `if let Ok(_)` form swallowed the
    /// lookup failure — defense-in-depth against future host callers that
    /// pass a name `find_user_function` didn't resolve.
    #[tokio::test]
    async fn validate_help_param_propagates_lookup_failure() {
        let eng = engine("function main() -> int { 1 }");
        let err = validate_help_param(&eng, "DoesNotExist").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("DoesNotExist"), "got: {msg}");
    }

    // ── build_args_from_signature: defaults / required / merge ──────────

    #[tokio::test]
    async fn build_args_primitive_via_cli() {
        let eng = engine(r#"function main(text: string) -> string { text }"#);
        let args = build_args_from_signature(
            &eng,
            &["--text".into(), "hi".into()],
            None,
            &["text".to_string()],
            &[ty_string()],
            &[false],
        )
        .await
        .unwrap();
        assert_eq!(args.len(), 1);
    }

    #[tokio::test]
    async fn build_args_missing_required_errors_with_hint() {
        let eng = engine(r#"function main(text: string) -> string { text }"#);
        let err = build_args_from_signature(
            &eng,
            &[],
            None,
            &["text".to_string()],
            &[ty_string()],
            &[false],
        )
        .await
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Missing required argument"), "got: {msg}");
        // Hint about `--` separator (added recently).
        assert!(msg.contains("--"), "got: {msg}");
    }

    /// BEP-027 §"What `baml pack` changes" — defaulted params allow the
    /// caller to omit them; the engine runs the default expression.
    #[tokio::test]
    async fn build_args_defaulted_param_yields_omitted() {
        let eng = engine(r#"function main(text: string, count: int = 10) -> string { text }"#);
        let args = build_args_from_signature(
            &eng,
            &["--text".into(), "hi".into()],
            None,
            &["text".to_string(), "count".to_string()],
            &[ty_string(), ty_int()],
            &[false, true],
        )
        .await
        .unwrap();
        assert_eq!(args.len(), 2);
        match &args[1] {
            BexCallArg::OmittedDefault => {}
            BexCallArg::Provided(_) => {
                panic!("expected OmittedDefault for `count`, got Provided")
            }
        }
    }

    /// BEP-027 §"JSON argument form": *"auto-CLI flags (after `--`)
    /// override JSON keys"*.
    #[tokio::test]
    async fn build_args_cli_overrides_json() {
        let eng = engine(r#"function main(text: string) -> string { text }"#);
        let json: serde_json::Value = serde_json::json!({"text": "from-json"});
        let args = build_args_from_signature(
            &eng,
            &["--text".into(), "from-cli".into()],
            Some(&json),
            &["text".to_string()],
            &[ty_string()],
            &[false],
        )
        .await
        .unwrap();
        match &args[0] {
            BexCallArg::Provided(v) => match v.as_ref() {
                BexExternalValue::String(s) => assert_eq!(s, "from-cli"),
                other => panic!("got {other:?}"),
            },
            BexCallArg::OmittedDefault => panic!("expected Provided, got OmittedDefault"),
        }
    }

    /// BEP-027 §"JSON argument form": *"top-level must be a JSON object"*.
    #[tokio::test]
    async fn build_args_json_args_must_be_object() {
        let eng = engine(r#"function main(text: string) -> string { text }"#);
        let json: serde_json::Value = serde_json::json!("just a string");
        let err = build_args_from_signature(
            &eng,
            &[],
            Some(&json),
            &["text".to_string()],
            &[ty_string()],
            &[false],
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("must be a JSON object"));
    }
}
