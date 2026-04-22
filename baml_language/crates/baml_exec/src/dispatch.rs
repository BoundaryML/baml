// `dispatch_target` is the shared entrypoint for both `baml run` and the
// `baml-host` runtime that packaged binaries ship with. Given an engine,
// a target function, a token stream (auto-CLI tokens, i.e. the stuff
// after `--` for `baml run` or `argv[2..]` for a packaged binary), and
// optional `--json-args` JSON, it:
//
//   1. Looks up the target's signature from the engine.
//   2. Parses auto-CLI tokens + merges with JSON args (auto-CLI wins).
//   3. Invokes the function.
//   4. Writes the return value to stdout in the configured format.
//
// Returns `Ok(true)` on success, `Ok(false)` on a caller-visible target
// error (e.g. missing argument, target runtime error), and propagates
// infrastructure errors via `Err`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context, Result, anyhow};
use bex_engine::{BexEngine, BexExternalValue, CallId, FunctionCallContextBuilder, Ty};

use crate::{
    auto_cli::parse_auto_cli_args,
    json_coerce::{json_to_external, json_to_external_with_ty},
    output::{OutputFormat, write_output},
};

/// Result of dispatching a target.
pub enum DispatchResult {
    /// Target completed successfully.
    Ok,
    /// Target raised an error (already printed to stderr).
    TargetError,
}

/// Invoke `target_name` with parameters drawn from `cli_tokens` (and
/// optionally `json_args`), then write the return value to stdout.
///
/// `cli_tokens` is the flag stream — under `baml run` these are the
/// tokens after `--`; under a packaged binary they are `argv[2..]`.
/// Parameterless targets ignore `cli_tokens` entirely and are invoked
/// with no arguments; argv is still visible to the function body via
/// `baml.argv` (owned by the caller-built engine, not this helper).
pub async fn dispatch_target(
    engine: Arc<BexEngine>,
    target_name: &str,
    cli_tokens: &[String],
    json_args: Option<serde_json::Value>,
    output_format: OutputFormat,
) -> Result<DispatchResult> {
    let func_info = engine
        .user_functions()
        .into_iter()
        .find(|f| {
            f.qualified_name == target_name
                || f.display_name == target_name.strip_prefix("user.").unwrap_or(target_name)
        })
        .ok_or_else(|| anyhow!("Function `{target_name}` not found"))?;

    let args = build_args_from_signature(
        cli_tokens,
        json_args.as_ref(),
        &func_info.param_names,
        &func_info.param_types,
    )?;

    let result = engine
        .call_function(
            target_name,
            args,
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(value) => {
            write_output(&value, output_format);
            Ok(DispatchResult::Ok)
        }
        Err(e) => {
            eprintln!("Error: {e}");
            Ok(DispatchResult::TargetError)
        }
    }
}

/// Build the ordered argument vector for a call by merging JSON args and
/// auto-CLI flags.
///
/// CLI flags override JSON keys — BEP-027 §"JSON argument form": "when
/// JSON and auto-CLI flags are both present, auto-CLI flags (after `--`)
/// override JSON keys".
pub fn build_args_from_signature(
    cli_tokens: &[String],
    json_args: Option<&serde_json::Value>,
    param_names: &[String],
    param_types: &[Ty],
) -> Result<Vec<BexExternalValue>> {
    let json_map = match json_args {
        Some(json) => {
            let obj = json
                .as_object()
                .ok_or_else(|| anyhow!("--json-args must be a JSON object, got: {json}"))?;
            let mut map = HashMap::new();
            for (key, value) in obj {
                let converted = match param_names.iter().position(|n| n == key) {
                    Some(idx) => json_to_external_with_ty(value, &param_types[idx])
                        .with_context(|| format!("--json-args: parameter `{key}`"))?,
                    None => json_to_external(value),
                };
                map.insert(key.clone(), converted);
            }
            map
        }
        None => HashMap::new(),
    };

    let cli_map = parse_auto_cli_args(cli_tokens, param_names, param_types)?;

    // CLI args override --json-args values.
    let mut merged = json_map;
    for (key, value) in cli_map {
        merged.insert(key, value);
    }

    let mut ordered = Vec::with_capacity(param_names.len());
    for (i, name) in param_names.iter().enumerate() {
        match merged.remove(name.as_str()) {
            Some(value) => ordered.push(value),
            None => {
                let ty = &param_types[i];
                anyhow::bail!("Missing required argument `--{name}` (type: {ty}).");
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
