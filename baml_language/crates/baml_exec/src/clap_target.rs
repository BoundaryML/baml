// Runtime clap parser for typed BAML entry points.
//
// Both `baml run --function X --` and the packed-binary host route their
// post-flag tokens through here. The win over the previous hand-rolled
// parser:
//   - Native clap `--help`/`-h`/`Usage:` block, styled with BAML's brand
//     purple via [`CLAP_STYLING`].
//   - Required-argument validation, unknown-flag detection, and "did you
//     mean" suggestions come for free.
//   - One renderer for help + parse errors across both call sites, so
//     `baml run --function llm.X -- --help` and `./packed-bin --help`
//     produce textually-identical output (modulo the binary name).
//
// Non-primitive parameters (class/list/map/union/media/alias/etc.) are
// not added as `--name` flags — they can't be faithfully expressed on the
// shell. Their only delivery channel is `--json-args`; the help block
// lists them as JSON keys so users see what shape the JSON object takes.

use std::collections::HashMap;

use anyhow::Result;
use bex_engine::{BexExternalValue, Ty, UserFunctionInfo};
use clap::{
    Arg, Command,
    builder::{PossibleValuesParser, styling},
};

use crate::auto_cli::{is_auto_cli_primitive, parse_cli_value};

/// Coerce a runtime [`String`] into a `&'static str` clap can consume.
///
/// The clap builder API takes `impl Into<Id>` / `impl Into<Str>` but only
/// implements those conversions from `&'static str`, not `String`. For
/// runtime-built commands the standard fix is to leak each name once —
/// the allocation lives for the rest of the process, which mirrors what
/// a compile-time clap derive does. The leak is bounded by the number of
/// parameters across all targets the host invokes; that's small.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Clap styling shared by every baml-cli surface. Lives in `baml_exec`
/// rather than `baml_cli::reporter` so packed binaries — which can't
/// depend on `baml_cli` — get the same look as the dev CLI.
pub const CLAP_STYLING: styling::Styles = {
    use clap::builder::styling::{AnsiColor, Color, Effects, RgbColor, Style, Styles};
    const PURPLE: Color = Color::Rgb(RgbColor(0xA8, 0x55, 0xF7));
    // Tonal pair — same hue family, pale (Tailwind purple-200). Used on
    // `<placeholders>` so they read as quiet secondary text against the
    // bold primary purple without washing out into background gray.
    const PURPLE_LIGHT: Color = Color::Rgb(RgbColor(0xE9, 0xD5, 0xFF));
    Styles::styled()
        .header(Style::new().fg_color(Some(PURPLE)).effects(Effects::BOLD))
        .usage(Style::new().fg_color(Some(PURPLE)).effects(Effects::BOLD))
        .literal(Style::new().effects(Effects::BOLD))
        .placeholder(Style::new().fg_color(Some(PURPLE_LIGHT)))
        .error(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Red)))
                .effects(Effects::BOLD),
        )
        .valid(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Green)))
                .effects(Effects::BOLD),
        )
        .invalid(
            Style::new()
                .fg_color(Some(Color::Ansi(AnsiColor::Yellow)))
                .effects(Effects::BOLD),
        )
};

/// Result of feeding raw tokens through the target's clap command.
#[derive(Debug, Default)]
pub struct ParsedTargetArgs {
    /// Per-parameter typed values pulled off the `--name <value>` flags.
    /// Empty if every parameter was delivered via `--json-args` (or the
    /// target has no parameters).
    pub cli_values: HashMap<String, BexExternalValue>,
    /// Raw `--json-args <SOURCE>` value (inline JSON, `@path`, or `-` for
    /// stdin) before [`crate::load_json_source`] turns it into a `Value`.
    pub json_source: Option<String>,
}

/// Run `tokens` through a clap [`Command`] built from `func_info`.
///
/// `bin_name` is the program identifier clap renders into `Usage:` (e.g.
/// `./packed-binary` or `baml run --function llm.X --`).
///
/// On help requests / parse errors, returns `Err(clap::Error)`; the
/// caller pattern-matches on `e.kind()` to decide stdout-vs-stderr and
/// the exit status. Successful parse returns a [`ParsedTargetArgs`].
pub fn parse_target_argv(
    bin_name: &str,
    function_name: &str,
    func_info: &UserFunctionInfo,
    tokens: &[String],
) -> Result<ParsedTargetArgs, clap::Error> {
    let cmd = build_target_command(bin_name, function_name, func_info);

    // Clap's `try_get_matches_from` treats the first element as argv[0]
    // — i.e. the binary name. Prepend `bin_name` so usage strings render
    // correctly without the caller having to remember.
    let argv: Vec<String> = std::iter::once(bin_name.to_string())
        .chain(tokens.iter().cloned())
        .collect();
    let matches = cmd.try_get_matches_from(argv)?;

    let mut cli_values = HashMap::new();
    for (name, ty) in func_info
        .param_names
        .iter()
        .zip(func_info.param_types.iter())
    {
        if !is_auto_cli_primitive(ty) {
            continue;
        }
        let Some(raw) = matches.get_one::<String>(name) else {
            continue;
        };
        // `parse_cli_value` is the same conversion the legacy parser
        // used; clap handled structural validation (required flags,
        // unknown flags, repeated flags) — semantic conversion of
        // primitives still lives here so enum/null/optional handling
        // doesn't get duplicated.
        let value = parse_cli_value(raw, ty).map_err(|e| {
            clap::Error::raw(
                clap::error::ErrorKind::ValueValidation,
                format!("invalid value for `--{name}`: {e}\n"),
            )
            .with_cmd(&build_target_command(bin_name, function_name, func_info))
        })?;
        cli_values.insert(name.clone(), value);
    }

    let json_source = matches.get_one::<String>("json-args").cloned();

    Ok(ParsedTargetArgs {
        cli_values,
        json_source,
    })
}

/// Build the clap [`Command`] that backs a typed target's CLI.
///
/// The shape:
///   `bin_name [OPTIONS]` (clap default) plus one `--name <TYPE>` arg per
///   primitive-typed parameter, plus an always-present `--json-args
///   <SOURCE>`. Optional positional `[VALUE]` enables single-required-
///   param positional sugar.
fn build_target_command(
    bin_name: &str,
    function_name: &str,
    func_info: &UserFunctionInfo,
) -> Command {
    let display = function_name.strip_prefix("user.").unwrap_or(function_name);
    let about = function_signature(display, func_info);

    // Clap's `Id` and `Str` types are constructible from `&'static str`,
    // not `String`. For runtime-built commands the standard escape hatch
    // is `Box::leak` — every command construction allocates O(params)
    // string bytes that survive the process. That's the same memory
    // pattern as a compile-time clap derive (one-time string allocation
    // per binary), just deferred to the first call.
    let mut cmd = Command::new(leak(bin_name.to_string()))
        .about(about)
        .styles(CLAP_STYLING)
        // Clap's default `help` subcommand makes no sense for a typed
        // target — entry points don't have subcommands.
        .disable_help_subcommand(true)
        // `--version` is a CLI affordance, not a target affordance; the
        // target's signature decides what flags are valid.
        .disable_version_flag(true);

    // Per-parameter `--name <TYPE>` flags for primitive types only.
    // Non-primitives have nowhere to live on argv and must arrive via
    // `--json-args`; the help block (after_help) tells the user.
    //
    // Positional values are intentionally not supported: every typed
    // parameter must arrive via its `--name` flag (or, for non-
    // primitives, via `--json-args`). The previous positional-sugar
    // shortcut was BEP-027's call but it makes signature changes
    // silently break callers — adding a second parameter flips a
    // working `./bin "hello"` into an unknown-arg error. Requiring
    // flags everywhere makes the call site self-documenting.
    for (idx, (name, ty)) in func_info
        .param_names
        .iter()
        .zip(func_info.param_types.iter())
        .enumerate()
    {
        if !is_auto_cli_primitive(ty) {
            continue;
        }
        let has_default = func_info
            .param_has_default
            .get(idx)
            .copied()
            .unwrap_or(false);
        let required = !has_default;
        let name_static: &'static str = leak(name.clone());
        let value_name: &'static str = leak(ty.to_string());
        let mut arg = Arg::new(name_static)
            .long(name_static)
            .value_name(value_name)
            .required(required);
        if has_default {
            arg = arg.help("[optional]");
        }
        // Validate finite-domain types at parse time so clap can offer
        // "did you mean" suggestions (`yes` -> `true`/`false`). Optional-
        // wrapped values stay on the generic String parser since "null"
        // also has to be accepted alongside the inner type.
        match ty {
            Ty::Bool { .. } => {
                arg = arg.value_parser(PossibleValuesParser::new(["true", "false"]));
            }
            Ty::Null { .. } => {
                arg = arg.value_parser(PossibleValuesParser::new(["null"]));
            }
            _ => {}
        }
        cmd = cmd.arg(arg);
    }

    // `--json-args` is universal: every target can accept it, and it's
    // the only delivery channel for non-primitive parameters.
    cmd = cmd.arg(
        Arg::new("json-args")
            .long("json-args")
            .value_name("SOURCE")
            .required(false)
            .help(
                "Pass arguments as JSON (inline string, `@path/to/file`, or `-` for stdin). \
                 Required for class/list/map/union parameters.",
            ),
    );

    // List non-primitive parameters in an `after_help` block so users
    // know what keys the `--json-args` JSON object must contain. Hidden
    // when every param is primitive (clap's auto-generated table already
    // covers those).
    if let Some(after_help) = json_only_params_block(func_info) {
        cmd = cmd.after_help(after_help);
    }

    cmd
}

/// `function llm.Summarize(text: string, max_words: int [optional]) -> string`
///
/// This is the `about` line that sits above clap's `Usage:` block. It's
/// the one piece of the legacy help text worth keeping — knowing what
/// you're about to call matters more than the table clap renders below.
fn function_signature(display: &str, func_info: &UserFunctionInfo) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    write!(out, "function {display}(").unwrap();
    let mut first = true;
    for (idx, (name, ty)) in func_info
        .param_names
        .iter()
        .zip(func_info.param_types.iter())
        .enumerate()
    {
        if !first {
            out.push_str(", ");
        }
        first = false;
        if func_info
            .param_has_default
            .get(idx)
            .copied()
            .unwrap_or(false)
        {
            write!(out, "{name}: {ty} [optional]").unwrap();
        } else {
            write!(out, "{name}: {ty}").unwrap();
        }
    }
    write!(out, ") -> {}", func_info.return_type).unwrap();
    out
}

/// Build an `after_help` block listing the parameters that must come
/// through `--json-args` (i.e. the non-primitives). Returns `None` when
/// every parameter is a primitive, since clap's auto-generated `Options:`
/// table already documents them.
///
/// TODO: this block currently renders as plain text below clap's
/// bold-purple `Usage:` / `Options:` headers, so it visually disconnects
/// from the rest of `--help`. Worse, when every param is non-primitive
/// the function-signature line in `about` already lists each one and
/// this block just restates it. Two improvements worth landing
/// together:
///   1. Style the `JSON-only parameters` header with `CLAP_STYLING`'s
///      `header` style and the type names with `placeholder`, so it
///      sits inside clap's visual palette instead of below it.
///   2. Suppress the block when every param is non-primitive — fall
///      back to the function-signature line which already shows them
///      all.
fn json_only_params_block(func_info: &UserFunctionInfo) -> Option<String> {
    use std::fmt::Write as _;
    let json_only: Vec<(&String, &Ty, bool)> = func_info
        .param_names
        .iter()
        .zip(func_info.param_types.iter())
        .enumerate()
        .filter(|(_, (_, ty))| !is_auto_cli_primitive(ty))
        .map(|(idx, (n, t))| {
            (
                n,
                t,
                func_info
                    .param_has_default
                    .get(idx)
                    .copied()
                    .unwrap_or(false),
            )
        })
        .collect();
    if json_only.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("JSON-only parameters (pass via --json-args):\n");
    for (name, ty, has_default) in json_only {
        if has_default {
            writeln!(out, "  {name}: {ty} [optional]").unwrap();
        } else {
            writeln!(out, "  {name}: {ty}").unwrap();
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use baml_type::{TyAttr, TypeName};
    use bex_engine::UserFunctionInfo;
    use bex_vm_types::types::FunctionOrigin;

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

    fn func_info(names: &[&str], types: Vec<Ty>, defaults: Vec<bool>, ret: Ty) -> UserFunctionInfo {
        UserFunctionInfo {
            qualified_name: "user.Test".into(),
            display_name: "Test".into(),
            origin: FunctionOrigin::UserDefined,
            param_names: names.iter().map(ToString::to_string).collect(),
            param_types: types,
            param_has_default: defaults,
            return_type: ret,
            source_file: String::new(),
            is_llm: false,
        }
    }

    /// All-primitive signature: every param becomes a `--name` flag and
    /// the clap parser binds typed values.
    #[test]
    fn parse_target_argv_primitive_flags() {
        let info = func_info(
            &["text", "n"],
            vec![ty_string(), ty_int()],
            vec![false, false],
            ty_string(),
        );
        let parsed = parse_target_argv(
            "./Test",
            "user.Test",
            &info,
            &["--text".into(), "hi".into(), "--n".into(), "5".into()],
        )
        .unwrap();
        assert_eq!(parsed.cli_values.len(), 2);
        assert!(parsed.json_source.is_none());
    }

    /// `--json-args` is recognized as a first-class flag — neither
    /// required nor mutually exclusive with `--name` flags.
    #[test]
    fn parse_target_argv_json_args_only() {
        let info = func_info(&["user"], vec![ty_class("User")], vec![false], ty_string());
        let parsed = parse_target_argv(
            "./Greet",
            "user.Greet",
            &info,
            &[
                "--json-args".into(),
                r#"{"user": {"name": "Avery"}}"#.into(),
            ],
        )
        .unwrap();
        assert!(parsed.cli_values.is_empty());
        assert_eq!(
            parsed.json_source.as_deref(),
            Some(r#"{"user": {"name": "Avery"}}"#)
        );
    }

    /// Missing required flag → clap surfaces `ErrorKind::MissingRequiredArgument`
    /// (not a free-form anyhow error). Callers pattern-match on that to
    /// route the message to stderr with the right styling.
    #[test]
    fn parse_target_argv_missing_required_flag() {
        let info = func_info(&["text"], vec![ty_string()], vec![false], ty_string());
        let err = parse_target_argv("./Test", "user.Test", &info, &[]).unwrap_err();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::MissingRequiredArgument,
            "got: {err}"
        );
    }

    /// Unknown flag → clap's "did you mean" path. Confirms we get the
    /// same suggestion engine as every other clap-driven baml verb.
    #[test]
    fn parse_target_argv_unknown_flag_errors() {
        let info = func_info(&["text"], vec![ty_string()], vec![false], ty_string());
        let err = parse_target_argv("./Test", "user.Test", &info, &["--txt".into(), "hi".into()])
            .unwrap_err();
        assert!(
            matches!(
                err.kind(),
                clap::error::ErrorKind::UnknownArgument | clap::error::ErrorKind::InvalidSubcommand
            ),
            "got: {err}"
        );
    }

    /// Bool flag rejects non-`true`/`false` via clap's `PossibleValuesParser`
    /// (gets the "valid values: true, false" hint for free).
    #[test]
    fn parse_target_argv_bool_invalid_value() {
        let info = func_info(&["flag"], vec![ty_bool()], vec![false], ty_string());
        let err = parse_target_argv(
            "./Test",
            "user.Test",
            &info,
            &["--flag".into(), "yes".into()],
        )
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    /// Bare positional tokens are rejected — every primitive param
    /// must arrive via its `--name` flag. Documents the deliberate
    /// drop of the legacy positional-sugar shortcut: a parameterless
    /// target that later gains a parameter would silently change
    /// behavior under positional sugar; requiring the flag makes
    /// signature changes visible at the call site.
    #[test]
    fn parse_target_argv_rejects_positional_token() {
        let info = func_info(&["text"], vec![ty_string()], vec![false], ty_string());
        let err = parse_target_argv("./Test", "user.Test", &info, &["hello".into()]).unwrap_err();
        // Clap surfaces this as either UnknownArgument (no positional
        // is registered) or InvalidSubcommand depending on version; both
        // are non-help, non-version error kinds that route to stderr.
        assert!(
            !matches!(
                err.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ),
            "got: {err}"
        );
    }

    /// Help request surfaces as `ErrorKind::DisplayHelp` so callers can
    /// print to stdout + exit 0 (vs. parse errors which go to stderr +
    /// non-zero).
    #[test]
    fn parse_target_argv_help_request_is_classified() {
        let info = func_info(&["text"], vec![ty_string()], vec![false], ty_string());
        let err = parse_target_argv("./Test", "user.Test", &info, &["--help".into()]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }
}
