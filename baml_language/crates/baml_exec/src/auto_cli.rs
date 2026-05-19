// Auto-CLI argument parser for typed BAML entry points.

#![allow(clippy::print_stdout)]

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use bex_engine::{BexExternalValue, Ty, UserFunctionInfo};

use crate::output::example_value;

/// Parse tokens into a map of parameter name → typed value.
///
/// `param_has_default` is parallel to `param_names`/`param_types` and is
/// consulted for the single-required-param positional-sugar branch (a
/// function with exactly one required parameter accepts a bare token,
/// regardless of how many other defaulted parameters it has).
///
/// BEP-027 §"Open questions" #5: class parameters can't be passed
/// through auto-CLI; `parse_cli_value` rejects structured types (Class,
/// List, Map, Union) with a pointer to `--json-args`.
pub fn parse_auto_cli_args(
    tokens: &[String],
    param_names: &[String],
    param_types: &[Ty],
    param_has_default: &[bool],
) -> Result<HashMap<String, BexExternalValue>> {
    if tokens.is_empty() || param_names.is_empty() {
        return Ok(HashMap::new());
    }

    // BEP-027 §"Calling a function: `--function`":
    //   # Single-required-param positional sugar
    //   baml run --function llm.Summarize -- "the text to summarize"
    //
    // The sugar fires when (a) there is exactly one bare token after `--`
    // and (b) exactly one parameter is *required* — defaulted parameters
    // are skipped so a function like `Summarize(text, max_words = 50,
    // style = "Concise")` still accepts a bare positional for `text`.
    if tokens.len() == 1 && !tokens[0].starts_with("--") {
        let required: Vec<usize> = (0..param_names.len())
            .filter(|i| !param_has_default.get(*i).copied().unwrap_or(false))
            .collect();
        if required.len() == 1 {
            let idx = required[0];
            let value = parse_cli_value(&tokens[0], &param_types[idx]).with_context(|| {
                format!("Invalid value for `{}`: {}", param_names[idx], tokens[0])
            })?;
            let mut map = HashMap::new();
            map.insert(param_names[idx].clone(), value);
            return Ok(map);
        }
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
            // A following `--`-prefixed token is the next flag, not the
            // value for this one — `--name --other=…` is a missing-value
            // typo, not a request to bind `name = "--other=…"`. Users who
            // want a literal value that starts with `--` use the
            // `--name=<value>` equals form (covered by
            // parse_auto_cli_args_equals_form_allows_dashed_value).
            if i >= tokens.len() || tokens[i].starts_with("--") {
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

fn find_param_index(key: &str, param_names: &[String]) -> Result<usize> {
    param_names.iter().position(|n| n == key).ok_or_else(|| {
        let available: Vec<&str> = param_names.iter().map(String::as_str).collect();
        anyhow!(
            "Unknown parameter `--{key}`.\nAvailable parameters: {}",
            available.join(", ")
        )
    })
}

/// Extract flag names from a token list, skipping bare tokens.
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

/// Convert a CLI string value to a typed [`BexExternalValue`] based on
/// the target type.
///
/// Structured types (`Class`, `List`, `Map`, `Union`) are rejected with a
/// pointer at `--json-args`, per BEP-027 §"Open questions" #5: class
/// parameters can't be passed through auto-CLI. `--json-args` is the
/// universal path (inline / `@file` / `-`) and the only one that
/// survives nontrivial shell quoting.
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

        // Per BEP-027 §"Open questions" #5: anything auto-CLI can't
        // faithfully represent must be delivered via `--json-args`.
        // Structured types (class/list/map/union) are the obvious case;
        // media, literals, type aliases, opaque types, and the engine-
        // internal types (function/void/watch/future) fall in the same
        // bucket — they either can't survive shell quoting or aren't
        // valid CLI parameter types. The previous catchall silently
        // String-coerced everything that fell through; that hides
        // genuine "this param can't be passed this way" errors behind a
        // confusing downstream type mismatch.
        _ => anyhow::bail!(
            "type `{ty}` can't be passed through auto-CLI; \
             deliver it via `--json-args '{{ \"<param>\": ... }}'` \
             (or `--json-args @file` / `--json-args -` for stdin)"
        ),
    }
}

/// Render per-target `--help` text from a function's signature, returning
/// a `String` the caller decides how to emit (print to stdout, send over
/// a socket, attach to a diagnostic, etc.).
///
/// `invocation_example` is the prefix shown in the example line. For
/// `baml run` pass something like `"baml run --function llm.X -- "` so
/// the example renders as `baml run --function llm.X -- --text "value"`.
/// For a packaged binary pass the binary path + a trailing space.
///
/// Renders `[optional]` markers for defaulted parameters and includes
/// only the *required* params in the usage example, matching the auto-CLI
/// model: the user can always supply optional flags, but only the
/// required ones must appear for the call to succeed.
pub fn target_help_text(
    function_name: &str,
    func_info: &UserFunctionInfo,
    invocation_example: &str,
) -> String {
    use std::fmt::Write as _;

    let display = function_name.strip_prefix("user.").unwrap_or(function_name);
    let param_names = &func_info.param_names;
    let param_types = &func_info.param_types;
    let param_has_default = &func_info.param_has_default;
    let ret_str = func_info.return_type.to_string();

    let params_str: Vec<String> = param_names
        .iter()
        .zip(param_types.iter())
        .enumerate()
        .map(|(idx, (n, t))| {
            if param_has_default.get(idx).copied().unwrap_or(false) {
                format!("{n}: {t} [optional]")
            } else {
                format!("{n}: {t}")
            }
        })
        .collect();

    let mut out = String::new();
    writeln!(
        out,
        "function {display}({}) -> {ret_str}",
        params_str.join(", ")
    )
    .unwrap();
    writeln!(out).unwrap();

    if param_names.is_empty() {
        writeln!(out, "  This function takes no arguments.").unwrap();
    } else {
        writeln!(out, "  Arguments:\n").unwrap();
        for (idx, (name, ty)) in param_names.iter().zip(param_types.iter()).enumerate() {
            let type_hint = match ty {
                Ty::Bool { .. } => " (use --name=true or --name=false)".to_string(),
                Ty::Enum(tn, _) => format!(" (enum {tn})"),
                Ty::Class(..) | Ty::Map { .. } | Ty::List(..) => " (pass JSON)".to_string(),
                _ => String::new(),
            };
            let optional = if param_has_default.get(idx).copied().unwrap_or(false) {
                " [optional]"
            } else {
                ""
            };
            writeln!(out, "    --{name} <{ty}>{optional}{type_hint}").unwrap();
        }
    }

    let example_args = param_names
        .iter()
        .zip(param_types.iter())
        .enumerate()
        .filter(|(idx, _)| !param_has_default.get(*idx).copied().unwrap_or(false))
        .map(|(_, (n, t))| format!("--{n} {}", example_value(t)))
        .collect::<Vec<_>>()
        .join(" ");
    writeln!(out).unwrap();
    if example_args.is_empty() {
        writeln!(out, "  Example: {invocation_example}").unwrap();
    } else {
        writeln!(out, "  Example: {invocation_example}{example_args}").unwrap();
    }

    out
}

/// Print [`target_help_text`] to stdout. Convenience wrapper for hosts
/// that just want the text on stdout (e.g. the pack-host binary).
pub fn print_target_help(
    function_name: &str,
    func_info: &UserFunctionInfo,
    invocation_example: &str,
) {
    print!(
        "{}",
        target_help_text(function_name, func_info, invocation_example)
    );
}

#[cfg(test)]
mod tests {
    use baml_type::{MediaKind, TyAttr, TypeName};

    use super::*;

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
    fn ty_bool() -> Ty {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> Ty {
        Ty::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_null() -> Ty {
        Ty::Null {
            attr: TyAttr::default(),
        }
    }
    fn ty_optional(inner: Ty) -> Ty {
        Ty::Optional(Box::new(inner), TyAttr::default())
    }
    fn ty_enum(name: &str) -> Ty {
        Ty::Enum(
            TypeName {
                name: name.into(),
                module_path: vec![],
                display_name: name.into(),
            },
            TyAttr::default(),
        )
    }
    fn ty_list(elem: Ty) -> Ty {
        Ty::List(Box::new(elem), TyAttr::default())
    }
    fn ty_class(name: &str) -> Ty {
        Ty::Class(
            TypeName {
                name: name.into(),
                module_path: vec![],
                display_name: name.into(),
            },
            vec![],
            TyAttr::default(),
        )
    }

    fn assert_string(raw: &BexExternalValue, expected: &str) {
        match raw {
            BexExternalValue::String(s) => assert_eq!(s, expected),
            other => panic!("expected String, got {other:?}"),
        }
    }

    // ── parse_cli_value: primitives ─────────────────────────────────────

    #[test]
    fn parse_cli_value_string_is_primitive() {
        assert_string(&parse_cli_value("hello", &ty_string()).unwrap(), "hello");
    }

    #[test]
    fn parse_cli_value_int_parses_signed() {
        let raw = parse_cli_value("-42", &ty_int()).unwrap();
        match raw {
            BexExternalValue::Int(v) => assert_eq!(v, -42),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parse_cli_value_int_rejects_non_integer() {
        assert!(parse_cli_value("3.14", &ty_int()).is_err());
        assert!(parse_cli_value("abc", &ty_int()).is_err());
    }

    #[test]
    fn parse_cli_value_float_parses() {
        let raw = parse_cli_value("2.5", &ty_float()).unwrap();
        match raw {
            BexExternalValue::Float(v) => {
                assert!((v - 2.5).abs() < 1e-9);
            }
            other => panic!("got {other:?}"),
        }
    }

    /// BEP-027 §"Auto-CLI conventions": booleans use `--flag=true` /
    /// `--flag=false`, not `--flag` / `--no-flag`.
    #[test]
    fn parse_cli_value_bool_requires_true_or_false() {
        assert!(matches!(
            parse_cli_value("true", &ty_bool()).unwrap(),
            BexExternalValue::Bool(true)
        ));
        assert!(matches!(
            parse_cli_value("false", &ty_bool()).unwrap(),
            BexExternalValue::Bool(false)
        ));
        assert!(parse_cli_value("yes", &ty_bool()).is_err());
        assert!(parse_cli_value("1", &ty_bool()).is_err());
    }

    #[test]
    fn parse_cli_value_null_accepts_literal_null_only() {
        assert!(matches!(
            parse_cli_value("null", &ty_null()).unwrap(),
            BexExternalValue::Null
        ));
        assert!(parse_cli_value("None", &ty_null()).is_err());
    }

    #[test]
    fn parse_cli_value_optional_null_is_null() {
        let raw = parse_cli_value("null", &ty_optional(ty_int())).unwrap();
        assert!(matches!(raw, BexExternalValue::Null));
    }

    #[test]
    fn parse_cli_value_optional_unwraps_inner() {
        let raw = parse_cli_value("7", &ty_optional(ty_int())).unwrap();
        assert!(matches!(raw, BexExternalValue::Int(7)));
    }

    /// BEP-027 §"Auto-CLI conventions": *"Enum values match the declared
    /// variant name exactly (case-sensitive)."* The parser just constructs
    /// the variant verbatim; runtime SAP rejects unknown variants.
    #[test]
    fn parse_cli_value_enum_constructs_variant_verbatim() {
        let raw = parse_cli_value("Concise", &ty_enum("Style")).unwrap();
        match raw {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                assert_eq!(enum_name, "Style");
                assert_eq!(variant_name, "Concise");
            }
            other => panic!("got {other:?}"),
        }
    }

    // ── parse_cli_value: complex types rejected (BEP-027 Open Q #5) ────

    /// Lists must be delivered via `--json-args`, not inline in auto-CLI.
    /// The error names `--json-args` so the user knows where to look.
    #[test]
    fn parse_cli_value_list_rejected_with_json_args_hint() {
        let err = parse_cli_value("[1, 2, 3]", &ty_list(ty_int())).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    /// Classes are the canonical case from BEP-027 Open Q #5.
    #[test]
    fn parse_cli_value_class_rejected_with_json_args_hint() {
        let err = parse_cli_value(r#"{"x": 1}"#, &ty_class("Foo")).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    /// Media (image / audio / video / pdf) can't survive shell quoting —
    /// previously the `_ =>` catchall silently String-coerced these,
    /// passing a raw filename through to the engine and producing a
    /// confusing downstream type mismatch. Reject up-front with the same
    /// `--json-args` pointer as structured types (BEP-027 Open Q #5).
    #[test]
    fn parse_cli_value_media_rejected_with_json_args_hint() {
        let ty = Ty::Media(MediaKind::Image, TyAttr::default());
        let err = parse_cli_value("/tmp/cat.png", &ty).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
        assert!(msg.contains("--json-args"), "got: {msg}");
    }

    /// Engine-internal types (`Function`) aren't valid auto-CLI inputs.
    /// The pre-fix catchall would silently produce `String(raw)` and let
    /// the engine fail later with a type mismatch; we now reject at the
    /// CLI boundary.
    #[test]
    fn parse_cli_value_engine_internal_type_rejected() {
        let ty = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            throws: Box::new(Ty::Void {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        };
        let err = parse_cli_value("anything", &ty).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("can't be passed through auto-CLI"),
            "got: {msg}"
        );
    }

    // ── parse_auto_cli_args: empty/named/positional/equals ──────────────

    #[test]
    fn parse_auto_cli_args_empty_tokens_yields_empty_map() {
        let out = parse_auto_cli_args(&[], &["x".to_string()], &[ty_int()], &[false]).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn parse_auto_cli_args_named_two_token_form() {
        let tokens = vec!["--name".to_string(), "value".to_string()];
        let out =
            parse_auto_cli_args(&tokens, &["name".to_string()], &[ty_string()], &[false]).unwrap();
        assert_eq!(out.len(), 1);
        assert_string(&out["name"], "value");
    }

    #[test]
    fn parse_auto_cli_args_named_equals_form() {
        let tokens = vec!["--name=value".to_string()];
        let out =
            parse_auto_cli_args(&tokens, &["name".to_string()], &[ty_string()], &[false]).unwrap();
        assert_string(&out["name"], "value");
    }

    /// `--name --other=...` is a missing-value typo, not a request to bind
    /// `name = "--other=..."`. Silently accepting the next flag as the
    /// value hides the real CLI mistake (especially for `string` params,
    /// where the type-coerce step doesn't reject it). To pass a literal
    /// value starting with `--`, users use the `--name=<value>` form.
    #[test]
    fn parse_auto_cli_args_rejects_following_flag_as_value() {
        let tokens = vec!["--name".to_string(), "--other=value".to_string()];
        let err = parse_auto_cli_args(
            &tokens,
            &["name".to_string(), "other".to_string()],
            &[ty_string(), ty_string()],
            &[false, false],
        )
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("Missing value for `--name`"), "got: {msg}");
    }

    /// Equals form is the escape hatch for values that start with `--`.
    #[test]
    fn parse_auto_cli_args_equals_form_allows_dashed_value() {
        let tokens = vec!["--name=--literal-dashes".to_string()];
        let out =
            parse_auto_cli_args(&tokens, &["name".to_string()], &[ty_string()], &[false]).unwrap();
        assert_string(&out["name"], "--literal-dashes");
    }

    #[test]
    fn parse_auto_cli_args_positional_sugar_single_param() {
        let tokens = vec!["hello".to_string()];
        let out =
            parse_auto_cli_args(&tokens, &["text".to_string()], &[ty_string()], &[false]).unwrap();
        assert_string(&out["text"], "hello");
    }

    /// BEP-027 §"Calling a function: `--function`" — single-required-
    /// param positional sugar: `Summarize(text: string, max_words: int =
    /// 50, style: SummaryStyle = "Concise")` accepts `-- "the text"` and
    /// binds the bare token to `text`.
    #[test]
    fn parse_auto_cli_args_positional_sugar_with_defaulted_params() {
        let tokens = vec!["the text to summarize".to_string()];
        let out = parse_auto_cli_args(
            &tokens,
            &[
                "text".to_string(),
                "max_words".to_string(),
                "style".to_string(),
            ],
            &[ty_string(), ty_int(), ty_string()],
            &[false, true, true],
        )
        .unwrap();
        assert_eq!(out.len(), 1, "only the required param is bound");
        assert_string(&out["text"], "the text to summarize");
    }

    /// Positional sugar does NOT fire when there are multiple required
    /// params — the user has to name them explicitly.
    #[test]
    fn parse_auto_cli_args_positional_sugar_skipped_when_multiple_required() {
        let tokens = vec!["x".to_string()];
        let out = parse_auto_cli_args(
            &tokens,
            &["a".to_string(), "b".to_string()],
            &[ty_string(), ty_string()],
            &[false, false],
        )
        .unwrap();
        // The bare token is skipped (it'll show up as a missing-required
        // error in build_args_from_signature) rather than guessed.
        assert!(out.is_empty(), "bare token without sugar must not bind");
    }

    /// BEP-027 §"Auto-CLI conventions": *"Unbound tokens after `--` pass
    /// through to `baml.argv` without binding."*
    #[test]
    fn parse_auto_cli_args_bare_tokens_skipped() {
        let tokens = vec![
            "--name".to_string(),
            "ada".to_string(),
            "extra".to_string(),
            "data".to_string(),
        ];
        let out =
            parse_auto_cli_args(&tokens, &["name".to_string()], &[ty_string()], &[false]).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out.contains_key("name"));
    }

    #[test]
    fn parse_auto_cli_args_unknown_flag_errors() {
        let tokens = vec!["--unknown".to_string(), "v".to_string()];
        let err = parse_auto_cli_args(&tokens, &["name".to_string()], &[ty_string()], &[false])
            .unwrap_err();
        assert!(format!("{err}").contains("Unknown parameter"));
    }

    #[test]
    fn parse_auto_cli_args_missing_value_for_flag_errors() {
        let tokens = vec!["--name".to_string()];
        let err = parse_auto_cli_args(&tokens, &["name".to_string()], &[ty_string()], &[false])
            .unwrap_err();
        assert!(format!("{err}").contains("Missing value"));
    }

    /// BEP-027 §"Auto-CLI conventions": *"Flag names mirror parameter
    /// names verbatim … No kebab translation."*
    #[test]
    fn parse_auto_cli_args_preserves_underscores_no_kebab_translation() {
        let tokens = vec!["--start_date".to_string(), "2024-01-01".to_string()];
        let out = parse_auto_cli_args(
            &tokens,
            &["start_date".to_string()],
            &[ty_string()],
            &[false],
        )
        .unwrap();
        assert!(out.contains_key("start_date"));
    }

    // ── extract_flag_keys ───────────────────────────────────────────────

    #[test]
    fn extract_flag_keys_handles_both_forms_and_skips_bare() {
        let tokens = vec![
            "--alpha".to_string(),
            "1".to_string(),
            "--beta=2".to_string(),
            "bare".to_string(),
            "--gamma".to_string(),
            "3".to_string(),
        ];
        assert_eq!(
            extract_flag_keys(&tokens),
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
    }

    #[test]
    fn extract_flag_keys_skips_empty_keys() {
        let tokens = vec!["--".to_string(), "--=foo".to_string()];
        assert!(extract_flag_keys(&tokens).is_empty());
    }
}
