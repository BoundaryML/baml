// Auto-CLI argument parser for typed BAML entry points.
//
// BEP-027: a target with a typed signature gets its flags derived from
// the signature. Flag names mirror parameter names verbatim
// (`start_date` → `--start_date`, no kebab translation). Booleans use
// `--flag=true`/`--flag=false`. Enum values match the declared variant
// name exactly (case-sensitive). For `baml run`, these tokens appear
// after `--`; for a packaged binary they appear top-level.

#![allow(clippy::print_stdout)]

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use bex_engine::{BexExternalValue, Ty, UserFunctionInfo};

use crate::{json_coerce::json_to_external_with_ty, output::example_value};

/// Parse tokens into a map of parameter name → value.
///
/// Supports:
/// - `--name value` (two tokens)
/// - `--name=value` (single token with `=`, including `--name=` for empty string)
/// - Positional sugar: single bare token when function has exactly one parameter
///
/// Bare tokens that don't match a `--flag` are skipped here — they remain
/// accessible via `baml.argv` but don't bind to parameters.
pub fn parse_auto_cli_args(
    tokens: &[String],
    param_names: &[String],
    param_types: &[Ty],
) -> Result<HashMap<String, BexExternalValue>> {
    if tokens.is_empty() || param_names.is_empty() {
        return Ok(HashMap::new());
    }

    // Positional sugar: single non-flag token + exactly one param.
    if tokens.len() == 1 && !tokens[0].starts_with("--") && param_names.len() == 1 {
        let value = parse_cli_value(&tokens[0], &param_types[0])
            .with_context(|| format!("Invalid value for `{}`: {}", param_names[0], tokens[0]))?;
        let mut map = HashMap::new();
        map.insert(param_names[0].clone(), value);
        return Ok(map);
    }

    let mut args = HashMap::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if !token.starts_with("--") {
            i += 1;
            continue;
        }
        let raw = &token[2..];

        let (key, val_str) = if let Some(eq_pos) = raw.find('=') {
            (&raw[..eq_pos], &raw[eq_pos + 1..])
        } else {
            i += 1;
            if i >= tokens.len() {
                anyhow::bail!("Missing value for `--{raw}`");
            }
            (raw, tokens[i].as_str())
        };

        let param_idx = find_param_index(key, param_names)?;
        let value = parse_cli_value(val_str, &param_types[param_idx])
            .with_context(|| format!("Invalid value for `--{key}`: {val_str}"))?;
        args.insert(key.to_string(), value);
        i += 1;
    }

    Ok(args)
}

/// Find parameter index by name, returning a helpful error if not found.
fn find_param_index(key: &str, param_names: &[String]) -> Result<usize> {
    param_names.iter().position(|n| n == key).ok_or_else(|| {
        let available: Vec<&str> = param_names.iter().map(String::as_str).collect();
        anyhow!(
            "Unknown parameter `--{key}`.\nAvailable parameters: {}",
            available.join(", ")
        )
    })
}

/// Extract flag names (`--key value` or `--key=value`) from a token list,
/// skipping bare (non-flag) tokens.
pub fn extract_flag_keys(tokens: &[String]) -> Vec<String> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if let Some(raw) = token.strip_prefix("--") {
            let key = raw.split('=').next().unwrap_or(raw);
            if !key.is_empty() {
                keys.push(key.to_string());
            }
            if !raw.contains('=') {
                i += 1;
            }
        }
        i += 1;
    }
    keys
}

/// Convert a CLI string value to a `BexExternalValue` based on the target type.
pub fn parse_cli_value(raw: &str, ty: &Ty) -> Result<BexExternalValue> {
    match ty {
        Ty::String { .. } => Ok(BexExternalValue::String(raw.to_string())),

        Ty::Int { .. } => {
            let v: i64 = raw
                .parse()
                .with_context(|| format!("Expected integer, got `{raw}`"))?;
            Ok(BexExternalValue::Int(v))
        }

        Ty::Float { .. } => {
            let v: f64 = raw
                .parse()
                .with_context(|| format!("Expected float, got `{raw}`"))?;
            Ok(BexExternalValue::Float(v))
        }

        Ty::Bool { .. } => match raw {
            "true" => Ok(BexExternalValue::Bool(true)),
            "false" => Ok(BexExternalValue::Bool(false)),
            _ => anyhow::bail!("Expected `true` or `false`, got `{raw}`"),
        },

        Ty::Null { .. } => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                anyhow::bail!("Expected `null`, got `{raw}`")
            }
        }

        Ty::Optional(inner, _) => {
            if raw == "null" {
                Ok(BexExternalValue::Null)
            } else {
                parse_cli_value(raw, inner)
            }
        }

        Ty::Enum(type_name, _) => Ok(BexExternalValue::Variant {
            enum_name: type_name.display_name.to_string(),
            variant_name: raw.to_string(),
        }),

        // Complex types accept inline JSON as a convenience; anything else
        // must go through `--json-args`.
        Ty::Class(..) | Ty::Map { .. } | Ty::List(..) | Ty::Union(..) => {
            match serde_json::from_str::<serde_json::Value>(raw) {
                Ok(json) => json_to_external_with_ty(&json, ty),
                Err(_) => anyhow::bail!(
                    "Parameter type `{ty}` requires JSON.\n\
                     Use `--json-args '{{...}}'` or pass a JSON string for this parameter."
                ),
            }
        }

        _ => Ok(BexExternalValue::String(raw.to_string())),
    }
}

/// Derive per-target `--help` output from a function's signature.
///
/// `invocation_example` is a caller-shaped usage example, e.g.
/// `"baml run --function llm.Summarize -- "` or `"./summarize "` — the
/// trailing space is preserved and the example parameters are appended.
pub fn print_target_help(
    function_name: &str,
    func_info: &UserFunctionInfo,
    invocation_example: &str,
) {
    let display = function_name.strip_prefix("user.").unwrap_or(function_name);
    let param_names = &func_info.param_names;
    let param_types = &func_info.param_types;
    let ret_str = func_info.return_type.to_string();

    let params_str: Vec<String> = param_names
        .iter()
        .zip(param_types.iter())
        .map(|(n, t)| format!("{n}: {t}"))
        .collect();

    println!("function {display}({}) -> {ret_str}", params_str.join(", "));
    println!();

    if param_names.is_empty() {
        println!("  This function takes no arguments.");
    } else {
        println!("  Arguments:\n");
        for (name, ty) in param_names.iter().zip(param_types.iter()) {
            let type_hint = match ty {
                Ty::Bool { .. } => " (use --name=true or --name=false)".to_string(),
                Ty::Enum(tn, _) => format!(" (enum {tn})"),
                Ty::Class(..) | Ty::Map { .. } | Ty::List(..) => " (pass JSON)".to_string(),
                _ => String::new(),
            };
            println!("    --{name} <{ty}>{type_hint}");
        }
    }

    println!();
    println!(
        "  Example: {invocation_example}{}",
        param_names
            .iter()
            .zip(param_types.iter())
            .map(|(n, t)| format!("--{n} {}", example_value(t)))
            .collect::<Vec<_>>()
            .join(" ")
    );
}
