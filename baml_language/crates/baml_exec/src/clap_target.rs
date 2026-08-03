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
use bex_engine::{BexExternalValue, RuntimeTy, UserFunctionInfo};
use clap::{
    Arg, ArgMatches, Command,
    builder::{PossibleValuesParser, styling},
};

use crate::{
    auto_cli::{is_auto_cli_primitive, parse_cli_value},
    envelope::TargetEntry,
};

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
    const ACCENT: Color = Color::Rgb(RgbColor(0xA8, 0x55, 0xF7));
    const PLACEHOLDER: Color = Color::Ansi(AnsiColor::Magenta);
    Styles::styled()
        .header(Style::new().fg_color(Some(ACCENT)).effects(Effects::BOLD))
        .usage(Style::new().fg_color(Some(ACCENT)).effects(Effects::BOLD))
        .literal(Style::new().effects(Effects::BOLD))
        .placeholder(Style::new().fg_color(Some(PLACEHOLDER)))
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

    let cli_values = extract_cli_values(func_info, &matches).map_err(|(name, err)| {
        clap::Error::raw(
            clap::error::ErrorKind::ValueValidation,
            format!("invalid value for `--{name}`: {err}\n"),
        )
        .with_cmd(&build_target_command(bin_name, function_name, func_info))
    })?;
    let json_source = matches.get_one::<String>("json-args").cloned();

    Ok(ParsedTargetArgs {
        cli_values,
        json_source,
    })
}

/// Multi-subcommand variant: `bin_name <subcommand> [OPTIONS]`. Each entry
/// in `targets` becomes one subcommand; the chosen subcommand's typed
/// args are decoded into a [`ParsedTargetArgs`].
///
/// `lookups` maps `qualified_name` → [`UserFunctionInfo`] for every entry
/// in `targets`. Caller is responsible for keeping the two in sync.
///
/// Returns `(chosen_qualified_name, ParsedTargetArgs)`. Help requests
/// (top-level or per-subcommand), unknown subcommands, unknown flags, and
/// missing required values all come back as `clap::Error`.
pub fn parse_multi_target_argv(
    bin_name: &str,
    targets: &[TargetEntry],
    lookups: &HashMap<String, UserFunctionInfo>,
    tokens: &[String],
) -> Result<(String, ParsedTargetArgs), clap::Error> {
    let cmd = build_multi_target_command(bin_name, targets, lookups);

    let argv: Vec<String> = std::iter::once(bin_name.to_string())
        .chain(tokens.iter().cloned())
        .collect();
    let matches = cmd.try_get_matches_from(argv)?;

    // `subcommand_required(true)` ensures clap rejects empty invocations
    // with `MissingSubcommand` before we reach here, so the `None` arm is
    // a defensive bail rather than a real reachable state.
    let (sub_name, sub_matches) = matches.subcommand().ok_or_else(|| {
        clap::Error::raw(
            clap::error::ErrorKind::MissingSubcommand,
            "no subcommand specified\n",
        )
    })?;

    let entry = targets
        .iter()
        .find(|t| t.subcommand_name == sub_name)
        .expect("clap matched a subcommand that wasn't registered");
    let info = lookups
        .get(&entry.qualified_name)
        .expect("lookups missing entry for registered subcommand");

    let cli_values = extract_cli_values(info, sub_matches).map_err(|(name, err)| {
        clap::Error::raw(
            clap::error::ErrorKind::ValueValidation,
            format!("invalid value for `--{name}`: {err}\n"),
        )
        .with_cmd(&build_multi_target_command(bin_name, targets, lookups))
    })?;
    let json_source = sub_matches.get_one::<String>("json-args").cloned();

    Ok((
        entry.qualified_name.clone(),
        ParsedTargetArgs {
            cli_values,
            json_source,
        },
    ))
}

/// Pull typed primitive values out of `matches` for a single target.
/// Returns `Err((param_name, message))` on conversion failure so the
/// caller can attach the proper `clap::Command` for nice rendering.
fn extract_cli_values(
    func_info: &UserFunctionInfo,
    matches: &ArgMatches,
) -> Result<HashMap<String, BexExternalValue>, (String, String)> {
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
        let value = parse_cli_value(raw, ty).map_err(|e| (name.clone(), format!("{e}")))?;
        cli_values.insert(name.clone(), value);
    }
    Ok(cli_values)
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
    build_target_command_with_name(bin_name, display, func_info).styles(CLAP_STYLING)
}

/// Per-target [`Command`] builder, parameterized on the command name so
/// it can be reused as both the top-level single-target command and a
/// subcommand of a multi-target parser. Styling is the caller's job —
/// the top-level command applies [`CLAP_STYLING`] once, which clap
/// propagates to every subcommand it owns.
fn build_target_command_with_name(
    cmd_name: &str,
    display_name: &str,
    func_info: &UserFunctionInfo,
) -> Command {
    let about = function_signature(display_name, func_info);

    // Clap's `Id` and `Str` types are constructible from `&'static str`,
    // not `String`. For runtime-built commands the standard escape hatch
    // is `Box::leak` — every command construction allocates O(params)
    // string bytes that survive the process. That's the same memory
    // pattern as a compile-time clap derive (one-time string allocation
    // per binary), just deferred to the first call.
    let mut cmd = Command::new(leak(cmd_name.to_string()))
        .about(about)
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
        let name_static: &'static str = leak(name.clone());
        let value_name: &'static str = leak(ty.to_string());
        let mut arg = Arg::new(name_static)
            .long(name_static)
            .value_name(value_name);
        // `--json-args` is an alternative delivery channel — a primitive
        // param is satisfied by `--json-args` even without its `--name`
        // flag, so require the flag only when neither `--json-args` nor
        // a default is in play. `dispatch` does the final "every param
        // bound?" check with a typed error if anything is missing.
        if !has_default {
            arg = arg.required_unless_present("json-args");
        } else {
            arg = arg.help("[optional]");
        }
        // Validate finite-domain types at parse time so clap can offer
        // "did you mean" suggestions (`yes` -> `true`/`false`). Optional-
        // wrapped values stay on the generic String parser since "null"
        // also has to be accepted alongside the inner type.
        match ty {
            RuntimeTy::Bool { .. } => {
                arg = arg.value_parser(PossibleValuesParser::new(["true", "false"]));
            }
            RuntimeTy::Null { .. } => {
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

/// Build a multi-subcommand clap [`Command`]: `bin_name <SUB>` with one
/// subcommand per [`TargetEntry`]. Each subcommand owns the same
/// per-parameter args as [`build_target_command_with_name`] would
/// produce, plus a per-subcommand `--json-args`.
fn build_multi_target_command(
    bin_name: &str,
    targets: &[TargetEntry],
    lookups: &HashMap<String, UserFunctionInfo>,
) -> Command {
    let mut root = Command::new(leak(bin_name.to_string()))
        .styles(CLAP_STYLING)
        // Every target is a subcommand; clap's auto-generated `help`
        // subcommand is fine here (unlike the single-target case) and
        // is the convention multi-subcommand CLIs follow.
        .disable_version_flag(true)
        // Empty invocations should print the top-level help and exit
        // non-zero, mirroring `git`/`cargo`. clap's combination of
        // `subcommand_required(true) + arg_required_else_help(true)`
        // does both.
        .subcommand_required(true)
        .arg_required_else_help(true);

    for entry in targets {
        let info = lookups
            .get(&entry.qualified_name)
            .expect("lookups must include every registered target");
        let sub = build_target_command_with_name(&entry.subcommand_name, &entry.display_name, info);
        root = root.subcommand(sub);
    }
    root
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
fn json_only_params_block(func_info: &UserFunctionInfo) -> Option<String> {
    use std::fmt::Write as _;
    let json_only: Vec<(&String, &RuntimeTy, bool)> = func_info
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

    fn ty_string() -> RuntimeTy {
        RuntimeTy::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_int() -> RuntimeTy {
        RuntimeTy::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> RuntimeTy {
        RuntimeTy::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_class(name: &str) -> RuntimeTy {
        RuntimeTy::Class(TypeName::local(name.into()), vec![], TyAttr::default())
    }

    fn func_info(
        names: &[&str],
        types: Vec<RuntimeTy>,
        defaults: Vec<bool>,
        ret: RuntimeTy,
    ) -> UserFunctionInfo {
        let display_param_types = types.iter().map(ToString::to_string).collect();
        let display_return_type = ret.to_string();
        UserFunctionInfo {
            qualified_name: "user.Test".into(),
            display_name: "Test".into(),
            origin: FunctionOrigin::UserDefined,
            param_names: names.iter().map(ToString::to_string).collect(),
            param_types: types,
            param_has_default: defaults,
            return_type: ret,
            display_type_params: Vec::new(),
            display_param_types,
            display_return_type,
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

    #[test]
    fn clap_help_uses_brand_accent_and_terminal_placeholders() {
        let info = func_info(&["text"], vec![ty_string()], vec![false], ty_string());
        let mut command = build_target_command("./Test", "user.Test", &info);
        let ansi = command.render_long_help().ansi().to_string();

        assert!(
            ansi.contains("\x1b[38;2;168;85;247m"),
            "missing brand accent: {ansi:?}"
        );
        assert!(
            ansi.contains("\x1b[35m"),
            "missing terminal magenta placeholder: {ansi:?}"
        );
        assert!(
            !ansi.contains("\x1b[38;2;233;213;255m"),
            "fixed pale placeholder in: {ansi:?}"
        );
        assert!(!ansi.contains("\x1b[38;5;"), "fixed 256-color in: {ansi:?}");
    }

    // ── Defaulted parameters ──────────────────────────────────────────

    /// A primitive param with `has_default = true` is omittable on the
    /// CLI — clap doesn't mark it required, and the parser succeeds even
    /// with no value supplied. The dispatch layer handles the omission
    /// downstream via `OmittedDefault`.
    #[test]
    fn parse_target_argv_defaulted_primitive_can_be_omitted() {
        let info = func_info(
            &["text", "count"],
            vec![ty_string(), ty_int()],
            vec![false, true], // count has a default
            ty_string(),
        );
        let parsed = parse_target_argv(
            "./Test",
            "user.Test",
            &info,
            &["--text".into(), "hi".into()],
        )
        .expect("missing defaulted `--count` should NOT error");
        // `count` is absent from cli_values — caller's dispatch will
        // turn that into `OmittedDefault` so the BAML default runs.
        assert!(parsed.cli_values.contains_key("text"));
        assert!(
            !parsed.cli_values.contains_key("count"),
            "defaulted-and-omitted params don't appear in cli_values"
        );
    }

    /// Defaulted param *passed explicitly* binds like any other primitive.
    #[test]
    fn parse_target_argv_defaulted_primitive_can_be_overridden() {
        let info = func_info(&["count"], vec![ty_int()], vec![true], ty_string());
        let parsed = parse_target_argv(
            "./Test",
            "user.Test",
            &info,
            &["--count".into(), "42".into()],
        )
        .unwrap();
        assert_eq!(parsed.cli_values.len(), 1);
    }

    /// The missing-required error names only the *non-defaulted* missing
    /// params; a defaulted param being absent isn't an error and isn't
    /// surfaced.
    #[test]
    fn parse_target_argv_missing_required_skips_defaulted() {
        let info = func_info(
            &["text", "count"],
            vec![ty_string(), ty_int()],
            vec![false, true],
            ty_string(),
        );
        let err = parse_target_argv("./Test", "user.Test", &info, &[]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        let rendered = format!("{err}");
        assert!(
            rendered.contains("--text"),
            "should name `--text`; got: {rendered}"
        );
        assert!(
            !rendered.contains("--count"),
            "must NOT cite defaulted `--count` as missing; got: {rendered}"
        );
    }

    /// Bool with default → CLI surfaces `[optional]` annotation in help
    /// and stays omittable. Behavioral check via successful parse with
    /// no `--flag` token.
    #[test]
    fn parse_target_argv_defaulted_bool_omittable() {
        let info = func_info(&["flag"], vec![ty_bool()], vec![true], ty_string());
        let parsed =
            parse_target_argv("./Test", "user.Test", &info, &[]).expect("optional bool ok");
        assert!(parsed.cli_values.is_empty());
    }

    /// Defaulted param + `--json-args` providing a value → `--json-args`
    /// carries the value to dispatch. The clap layer doesn't see it (we
    /// stash `json_source` for dispatch to merge), but the parse must
    /// succeed without `--name` being supplied either.
    #[test]
    fn parse_target_argv_defaulted_satisfied_by_json_args() {
        let info = func_info(&["count"], vec![ty_int()], vec![true], ty_string());
        let parsed = parse_target_argv(
            "./Test",
            "user.Test",
            &info,
            &["--json-args".into(), r#"{"count": 42}"#.into()],
        )
        .unwrap();
        assert!(parsed.cli_values.is_empty());
        assert_eq!(
            parsed.json_source.as_deref(),
            Some(r#"{"count": 42}"#),
            "json_source should carry through verbatim"
        );
    }

    // ── Defaulted params on the multi-subcommand parser ─────────────

    /// Subcommand variant of `defaulted_primitive_can_be_omitted` — a
    /// defaulted param under a subcommand can be left off, and the
    /// chosen subcommand's other required params are still validated.
    #[test]
    fn parse_multi_target_argv_defaulted_param_under_subcommand_omittable() {
        let mut info = func_info(
            &["text", "count"],
            vec![ty_string(), ty_int()],
            vec![false, true],
            ty_string(),
        );
        info.qualified_name = "user.summarize".into();
        info.display_name = "summarize".into();
        let entry = TargetEntry {
            qualified_name: "user.summarize".into(),
            display_name: "summarize".into(),
            subcommand_name: "summarize".into(),
        };
        let mut lookups = HashMap::new();
        lookups.insert("user.summarize".to_string(), info);

        let (chosen, parsed) = parse_multi_target_argv(
            "./cli",
            &[entry],
            &lookups,
            &["summarize".into(), "--text".into(), "hi".into()],
        )
        .expect("defaulted `count` should be omittable");
        assert_eq!(chosen, "user.summarize");
        assert!(parsed.cli_values.contains_key("text"));
        assert!(!parsed.cli_values.contains_key("count"));
    }

    /// Subcommand with a required param missing while a defaulted param
    /// is also absent → error names only the required one.
    #[test]
    fn parse_multi_target_argv_missing_required_under_subcommand_names_only_required() {
        let mut info = func_info(
            &["text", "count"],
            vec![ty_string(), ty_int()],
            vec![false, true],
            ty_string(),
        );
        info.qualified_name = "user.summarize".into();
        info.display_name = "summarize".into();
        let entry = TargetEntry {
            qualified_name: "user.summarize".into(),
            display_name: "summarize".into(),
            subcommand_name: "summarize".into(),
        };
        let mut lookups = HashMap::new();
        lookups.insert("user.summarize".to_string(), info);

        let err = parse_multi_target_argv("./cli", &[entry], &lookups, &["summarize".into()])
            .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
        let rendered = format!("{err}");
        assert!(rendered.contains("--text"), "got: {rendered}");
        assert!(
            !rendered.contains("--count"),
            "defaulted param must not appear in the missing-required list; got: {rendered}"
        );
    }

    /// `--json-args` alone satisfies a primitive parameter — the
    /// `required_unless_present` flag wiring means we don't get a
    /// false-positive `MissingRequiredArgument`. Regression test for the
    /// bug where `./cli describe --json-args='{"ty":"x"}'` was rejected.
    #[test]
    fn parse_target_argv_primitive_satisfied_by_json_args() {
        let info = func_info(&["ty"], vec![ty_string()], vec![false], ty_string());
        let parsed = parse_target_argv(
            "./Test",
            "user.Test",
            &info,
            &["--json-args".into(), r#"{"ty":"x"}"#.into()],
        )
        .expect("--json-args alone should satisfy primitive param");
        // Primitive isn't on the CLI side; the value flows in through
        // json_source and `dispatch` decodes it later.
        assert!(parsed.cli_values.is_empty());
        assert_eq!(parsed.json_source.as_deref(), Some(r#"{"ty":"x"}"#));
    }

    // ── parse_multi_target_argv ───────────────────────────────────────

    /// Helper: build a `TargetEntry` + matching `func_info` pair for one
    /// function. Subcommand name defaults to the display name's last
    /// `.`-segment, matching `pack_command`'s `resolve_one`.
    fn target_entry(
        qualified: &str,
        info_param_names: &[&str],
        info_param_types: Vec<RuntimeTy>,
        info_param_defaults: Vec<bool>,
    ) -> (TargetEntry, UserFunctionInfo) {
        let display = qualified.strip_prefix("user.").unwrap_or(qualified);
        let sub = display.rsplit('.').next().unwrap_or(display);
        let mut info = func_info(
            info_param_names,
            info_param_types,
            info_param_defaults,
            ty_string(),
        );
        info.qualified_name = qualified.to_string();
        info.display_name = display.to_string();
        let entry = TargetEntry {
            qualified_name: qualified.to_string(),
            display_name: display.to_string(),
            subcommand_name: sub.to_string(),
        };
        (entry, info)
    }

    /// Take ownership of the target pairs and split into the two shapes
    /// `parse_multi_target_argv` wants: an ordered `entries` slice and a
    /// `lookups` map keyed on qualified name. Returning by value avoids
    /// the `.clone()`-per-pair pattern at call sites.
    fn split_targets(
        pairs: Vec<(TargetEntry, UserFunctionInfo)>,
    ) -> (Vec<TargetEntry>, HashMap<String, UserFunctionInfo>) {
        let mut entries = Vec::with_capacity(pairs.len());
        let mut lookups = HashMap::with_capacity(pairs.len());
        for (entry, info) in pairs {
            lookups.insert(info.qualified_name.clone(), info);
            entries.push(entry);
        }
        (entries, lookups)
    }

    /// Happy path: two subcommands, user picks one, typed value comes
    /// back bound to that subcommand's signature.
    #[test]
    fn parse_multi_target_argv_dispatches_to_subcommand() {
        let (entries, lookups) = split_targets(vec![
            target_entry("user.describe", &["ty"], vec![ty_string()], vec![false]),
            target_entry(
                "user.greet",
                &["name", "excited"],
                vec![ty_string(), ty_bool()],
                vec![false, false],
            ),
        ]);

        let (chosen, parsed) = parse_multi_target_argv(
            "./cli",
            &entries,
            &lookups,
            &[
                "greet".into(),
                "--name".into(),
                "Avery".into(),
                "--excited".into(),
                "true".into(),
            ],
        )
        .expect("greet subcommand should parse");
        assert_eq!(chosen, "user.greet");
        assert_eq!(parsed.cli_values.len(), 2);
        assert!(parsed.json_source.is_none());
    }

    /// No subcommand → `arg_required_else_help(true)` fires as
    /// `DisplayHelpOnMissingArgumentOrSubcommand`, NOT `DisplayHelp`.
    /// The host classifies this as exit-non-zero (vs. user-asked `-h`
    /// which is exit-0).
    #[test]
    fn parse_multi_target_argv_empty_invocation_is_not_help_request() {
        let (entries, lookups) = split_targets(vec![target_entry(
            "user.describe",
            &["ty"],
            vec![ty_string()],
            vec![false],
        )]);
        let err = parse_multi_target_argv("./cli", &entries, &lookups, &[]).unwrap_err();
        assert!(
            !matches!(err.kind(), clap::error::ErrorKind::DisplayHelp),
            "empty must not classify as a help request (would route to exit 0); got: {err}"
        );
    }

    /// User typed `-h` after the subcommand → that subcommand's help
    /// renders (`DisplayHelp`), distinct from the no-subcommand case.
    #[test]
    fn parse_multi_target_argv_subcommand_help_is_display_help() {
        let (entries, lookups) = split_targets(vec![target_entry(
            "user.describe",
            &["ty"],
            vec![ty_string()],
            vec![false],
        )]);
        let err = parse_multi_target_argv(
            "./cli",
            &entries,
            &lookups,
            &["describe".into(), "--help".into()],
        )
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    /// Unknown subcommand → clap's `InvalidSubcommand` (with did-you-mean).
    #[test]
    fn parse_multi_target_argv_unknown_subcommand_errors() {
        let (entries, lookups) = split_targets(vec![target_entry(
            "user.describe",
            &["ty"],
            vec![ty_string()],
            vec![false],
        )]);
        let err =
            parse_multi_target_argv("./cli", &entries, &lookups, &["greet".into()]).unwrap_err();
        assert!(
            matches!(
                err.kind(),
                clap::error::ErrorKind::InvalidSubcommand | clap::error::ErrorKind::UnknownArgument
            ),
            "got: {err}"
        );
    }

    /// `--json-args` after the subcommand alone satisfies primitive
    /// params. Same `required_unless_present` regression coverage as the
    /// single-target variant, but on the multi-target parser.
    #[test]
    fn parse_multi_target_argv_json_args_satisfies_primitive() {
        let (entries, lookups) = split_targets(vec![target_entry(
            "user.describe",
            &["ty"],
            vec![ty_string()],
            vec![false],
        )]);
        let (chosen, parsed) = parse_multi_target_argv(
            "./cli",
            &entries,
            &lookups,
            &[
                "describe".into(),
                "--json-args".into(),
                r#"{"ty":"x"}"#.into(),
            ],
        )
        .expect("--json-args alone should satisfy `ty`");
        assert_eq!(chosen, "user.describe");
        assert!(parsed.cli_values.is_empty());
        assert_eq!(parsed.json_source.as_deref(), Some(r#"{"ty":"x"}"#));
    }

    /// Missing required *under* the subcommand (no `--json-args`, no
    /// `--ty`) → `MissingRequiredArgument`. Confirms the per-subcommand
    /// required check still fires when `--json-args` isn't filling in.
    #[test]
    fn parse_multi_target_argv_missing_required_under_subcommand() {
        let (entries, lookups) = split_targets(vec![target_entry(
            "user.describe",
            &["ty"],
            vec![ty_string()],
            vec![false],
        )]);
        let err =
            parse_multi_target_argv("./cli", &entries, &lookups, &["describe".into()]).unwrap_err();
        assert_eq!(
            err.kind(),
            clap::error::ErrorKind::MissingRequiredArgument,
            "got: {err}"
        );
    }

    /// Each subcommand owns its own `--name` namespace — passing
    /// subcommand A's flag inside subcommand B is an unknown-flag error,
    /// not a silent accept.
    #[test]
    fn parse_multi_target_argv_subcommand_flags_are_isolated() {
        let (entries, lookups) = split_targets(vec![
            target_entry("user.describe", &["ty"], vec![ty_string()], vec![false]),
            target_entry(
                "user.greet",
                &["name", "excited"],
                vec![ty_string(), ty_bool()],
                vec![false, false],
            ),
        ]);
        let err = parse_multi_target_argv(
            "./cli",
            &entries,
            &lookups,
            &["describe".into(), "--name".into(), "Avery".into()],
        )
        .unwrap_err();
        assert!(
            matches!(
                err.kind(),
                clap::error::ErrorKind::UnknownArgument | clap::error::ErrorKind::InvalidSubcommand
            ),
            "describe shouldn't accept greet's `--name`; got: {err}"
        );
    }

    /// Verb-level (pre-subcommand) `--json-args` is NOT accepted — the
    /// spec puts json args inside the chosen subcommand, not at root.
    /// This test pins the design so a future "global args" rework
    /// doesn't quietly add it.
    #[test]
    fn parse_multi_target_argv_does_not_accept_root_json_args() {
        let (entries, lookups) = split_targets(vec![target_entry(
            "user.describe",
            &["ty"],
            vec![ty_string()],
            vec![false],
        )]);
        let err = parse_multi_target_argv(
            "./cli",
            &entries,
            &lookups,
            &[
                "--json-args".into(),
                r#"{"ty":"x"}"#.into(),
                "describe".into(),
            ],
        )
        .unwrap_err();
        assert!(
            !matches!(err.kind(), clap::error::ErrorKind::DisplayHelp),
            "root `--json-args` should be a parse error, not help; got: {err}"
        );
    }
}
