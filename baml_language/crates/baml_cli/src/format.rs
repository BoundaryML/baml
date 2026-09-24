use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use baml_db::{
    ProjectDatabase, SourceRootKind, SourceRootSpec, baml_compiler_lexer, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNode},
    discover_baml_files,
};
use baml_fmt::FormatOptions;
use clap::Args;

use crate::{project_load::resolve_project_layout, reporter::Reporter};

/// Format BAML source files.
///
/// With explicit paths, formats those files or directories. With no paths,
/// discovers the nearest BAML project and formats all of its `.baml` files.
/// If no project is found, the command succeeds without changing anything.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Format the nearest project:
    baml fmt

  Format a specific file:
    baml fmt baml_src/main.baml

  Preview formatted output:
    baml fmt --dry-run

  Migrate removed hash strings without changing their values:
    baml fmt --fix-removed-features")]
pub struct FormatArgs {
    #[arg(
        help = "Specific files to format. If omitted, all `.baml` files in the project are formatted."
    )]
    pub paths: Vec<PathBuf>,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    #[arg(
        short = 'n',
        long = "dry-run",
        help = "Write formatter changes to stdout instead of files.",
        default_value = "false",
        help_heading = "Output options"
    )]
    pub dry_run: bool,

    #[arg(
        long,
        help = "Rewrite removed non-template hash strings without changing their values",
        long_help = "Rewrite removed non-template hash strings to render-equivalent quoted strings. Legacy Jinja prompts and template_string bodies require manual migration to backtick templates.",
        default_value = "false"
    )]
    pub fix_removed_features: bool,
}

impl FormatArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        // Cargo-style default: with no positional paths, discover every
        // `.baml` file under the project root and format the lot. The
        // project-marker rule keeps `baml fmt` from silently rewriting
        // every `.baml` under cwd from an unrelated directory; with no
        // marker there's simply nothing to format (a no-op success).
        let paths = if self.paths.is_empty() {
            match discover_project_files(self.from.as_deref())? {
                Some(files) => files,
                None => {
                    // No `baml.toml` / `baml_src/` here — there's nothing to
                    // format, so don't fail. A no-op success beats a hard
                    // error for a command agents run reflexively; pass
                    // explicit file paths to format loose `.baml` files.
                    Reporter::new().finish("Finished", "no BAML project found; nothing to format");
                    return Ok(crate::ExitCode::Success);
                }
            }
        } else {
            expand_explicit_paths(&self.paths)
        };

        if paths.is_empty() {
            let search_root = if self.paths.is_empty() {
                self.from
                    .as_deref()
                    .unwrap_or_else(|| Path::new("."))
                    .display()
                    .to_string()
            } else {
                self.paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            crate::reporter::print_error(format_args!("no .baml files found in {}", search_root));
            return Ok(crate::ExitCode::Other);
        }

        // In dry-run mode the formatted source goes to stdout — don't
        // start a spinner that would compete with it. Outside dry-run
        // we get the standard cargo-style verb sequence (one
        // `Formatting <path>` per file, persisting to scrollback like
        // cargo's `Compiling foo v0.1.0` lines).
        let reporter = if self.dry_run {
            None
        } else {
            Some(Reporter::new())
        };

        let mut num_failures: usize = 0;
        for path in &paths {
            if let Some(r) = &reporter {
                r.spin("Formatting", path.display().to_string());
            }
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(err) => {
                    crate::reporter::print_error(format_args!(
                        "failed to read {}: {err}",
                        path.display()
                    ));
                    num_failures += 1;
                    continue;
                }
            };
            let source = if self.fix_removed_features && !baml_fmt::has_ignore_directive(&source) {
                match migrate_removed_features(&source) {
                    Ok(source) => source,
                    Err(err) => {
                        crate::reporter::print_error(format_args!(
                            "migrating {}: {err}",
                            path.display()
                        ));
                        num_failures += 1;
                        continue;
                    }
                }
            } else {
                source
            };
            let options = FormatOptions::default();
            match baml_fmt::format(&source, &options) {
                Ok(formatted) => {
                    if self.dry_run {
                        #[allow(clippy::print_stdout)]
                        {
                            println!("{formatted}");
                        }
                    } else if let Err(err) = fs::write(path, formatted) {
                        crate::reporter::print_error(format_args!(
                            "failed to write formatted source to {}: {err}",
                            path.display()
                        ));
                        num_failures += 1;
                    }
                }
                Err(err) => {
                    match err {
                        baml_fmt::FormatterError::ParseErrors(err) => {
                            crate::reporter::print_error(format_args!(
                                "formatting {}: {err:?}",
                                path.display()
                            ));
                        }
                        baml_fmt::FormatterError::StrongAstError(err) => {
                            let err = err.print_with_file_context(path, &source);
                            crate::reporter::print_error(format_args!("while formatting: {err}"));
                        }
                    }
                    num_failures += 1;
                }
            }
        }

        let total = paths.len();
        let ok = total - num_failures;
        if num_failures > 0 {
            if let Some(r) = &reporter {
                r.abandon();
            }
            crate::reporter::print_error(format_args!(
                "formatted {ok} of {total} file(s); {num_failures} failed"
            ));
            Ok(crate::ExitCode::Other)
        } else {
            if let Some(r) = &reporter {
                r.finish("Finished", format!("formatted {ok} file(s)"));
            }
            Ok(crate::ExitCode::Success)
        }
    }
}

/// Rewrite removed non-template hash string literals to byte-equivalent quoted strings.
///
/// Compiler2 lowered ordinary hash-string bodies verbatim, without escape decoding or dedenting, so every character in the body must be escaped for the quoted-string decoder. Legacy Jinja prompt and `template_string` bodies are deliberately left untouched: quoted strings would make their interpolation inert, and migrating Jinja to BEP-049 templates requires a separate semantic transformation.
fn migrate_removed_features(
    source: &str,
) -> std::result::Result<String, RemovedFeatureMigrationError> {
    let mut db = ProjectDatabase::new();
    let root = db
        .add_source_root(SourceRootSpec::new(
            "<fmt-migration>",
            SourceRootKind::Workspace,
        ))
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
    let source_file = db.add_or_update_file_in(
        root,
        &PathBuf::from("<fmt-migration>").join("file.baml"),
        source,
    );
    let tokens = baml_compiler_lexer::lex_file(&db, source_file);
    let (parsed, _errors) = baml_compiler_parser::parse_file(&tokens);
    let cst = SyntaxNode::new_root(parsed);

    let mut has_legacy_jinja_template = false;
    let mut replacements = cst
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::RAW_STRING_LITERAL)
        .filter_map(|node| {
            let start = node
                .children_with_tokens()
                .filter_map(SyntaxElement::into_token)
                .find(|token| token.kind() == SyntaxKind::HASH)?
                .text_range()
                .start();
            let start = usize::from(start);
            let end = usize::from(node.text_range().end());
            let replacement = hash_string_to_quoted(source.get(start..end)?)?;
            if node.ancestors().skip(1).any(|ancestor| {
                matches!(
                    ancestor.kind(),
                    SyntaxKind::PROMPT_FIELD | SyntaxKind::TEMPLATE_STRING_DEF
                )
            }) {
                has_legacy_jinja_template = true;
                return None;
            }
            Some((start..end, replacement))
        })
        .collect::<Vec<_>>();

    if has_legacy_jinja_template {
        return Err(RemovedFeatureMigrationError::LegacyJinjaTemplate);
    }

    // Apply from the end so earlier CST byte offsets remain valid.
    replacements.sort_by_key(|(range, _)| range.start);
    let mut migrated = source.to_string();
    for (range, replacement) in replacements.into_iter().rev() {
        migrated.replace_range(range, &replacement);
    }
    Ok(migrated)
}

#[derive(Debug, PartialEq, Eq)]
enum RemovedFeatureMigrationError {
    LegacyJinjaTemplate,
}

impl std::fmt::Display for RemovedFeatureMigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LegacyJinjaTemplate => write!(
                f,
                "legacy Jinja prompts and `template_string` bodies require manual migration to backtick templates with `${{...}}` interpolation"
            ),
        }
    }
}

fn hash_string_to_quoted(raw: &str) -> Option<String> {
    let hash_count = raw.bytes().take_while(|byte| *byte == b'#').count();
    if hash_count == 0 || raw.as_bytes().get(hash_count) != Some(&b'"') {
        return None;
    }

    let body_start = hash_count + 1;
    let body_end = raw.len().checked_sub(hash_count + 1)?;
    if body_end < body_start
        || raw.as_bytes().get(body_end) != Some(&b'"')
        || !raw[body_end + 1..].bytes().all(|byte| byte == b'#')
    {
        return None;
    }

    let body = raw.get(body_start..body_end)?;
    let mut quoted = String::with_capacity(body.len() + 2);
    quoted.push('"');
    for ch in body.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            '\0' => quoted.push_str("\\0"),
            '\u{0008}' => quoted.push_str("\\b"),
            '\u{000B}' => quoted.push_str("\\v"),
            '\u{000C}' => quoted.push_str("\\f"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    Some(quoted)
}

fn expand_explicit_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut expanded = Vec::new();

    for path in paths {
        if path.is_dir() {
            expanded.extend(discover_baml_files(path));
        } else {
            expanded.push(path.clone());
        }
    }

    let mut seen = HashSet::new();
    expanded.retain(|path| seen.insert(path.clone()));
    expanded
}

/// Walk a resolved source root and return every `.baml` file inside it.
/// Omitted `--from` requires a `baml.toml` or `baml_src/` marker so `baml fmt`
/// doesn't accidentally rewrite every `.baml` below an unrelated cwd.
/// An explicit `--from` is itself a safe opt-in to format that source tree.
///
/// Returns `Ok(None)` when neither marker is present. The caller turns `None`
/// into a no-op success rather than a hard error.
fn discover_project_files(from: Option<&Path>) -> Result<Option<Vec<PathBuf>>> {
    let Some(layout) = resolve_project_layout(from)? else {
        return Ok(None);
    };
    Ok(Some(discover_baml_files(&layout.source_root)))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn explicit_directory_formats_baml_files_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        let baml_src = tmp.path().join("baml_src");
        let nested_dir = baml_src.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();

        let main = baml_src.join("main.baml");
        let nested = nested_dir.join("nested.baml");
        let ignored = nested_dir.join("ignored.txt");
        let main_source = "function main() -> string { \"hello\" }\n";
        let nested_source = "function nested() -> int { 1 }\n";
        let ignored_source = "function ignored() -> int { 1 }\n";

        fs::write(&main, main_source).unwrap();
        fs::write(&nested, nested_source).unwrap();
        fs::write(&ignored, ignored_source).unwrap();

        let args = FormatArgs {
            paths: vec![baml_src],
            from: Some(tmp.path().to_path_buf()),
            dry_run: false,
            fix_removed_features: false,
        };
        let exit_code = args.run().unwrap();

        assert!(matches!(exit_code, crate::ExitCode::Success));
        assert_eq!(
            fs::read_to_string(&main).unwrap(),
            baml_fmt::format(main_source, &FormatOptions::default()).unwrap()
        );
        assert_eq!(
            fs::read_to_string(&nested).unwrap(),
            baml_fmt::format(nested_source, &FormatOptions::default()).unwrap()
        );
        assert_eq!(fs::read_to_string(ignored).unwrap(), ignored_source);
    }

    #[test]
    fn explicit_overlapping_paths_are_deduplicated() {
        let tmp = tempfile::tempdir().unwrap();
        let baml_src = tmp.path().join("baml_src");
        let nested_dir = baml_src.join("nested");
        fs::create_dir_all(&nested_dir).unwrap();

        let main = baml_src.join("main.baml");
        let nested = nested_dir.join("nested.baml");
        fs::write(&main, "function main() -> string { \"hello\" }\n").unwrap();
        fs::write(&nested, "function nested() -> int { 1 }\n").unwrap();

        let expanded = expand_explicit_paths(&[
            baml_src.clone(),
            main.clone(),
            nested_dir.clone(),
            nested.clone(),
        ]);

        assert_eq!(expanded, vec![main, nested]);
    }

    #[test]
    fn fix_removed_features_migrates_hash_strings_before_formatting() {
        let tmp = tempfile::tempdir().unwrap();
        let source_file = tmp.path().join("legacy.baml");
        fs::write(
            &source_file,
            "function legacy() -> string {\n    #\"\n    first\n    second\n\"#\n}\n",
        )
        .unwrap();

        let args = FormatArgs {
            paths: vec![source_file.clone()],
            from: None,
            dry_run: false,
            fix_removed_features: true,
        };
        let exit_code = args.run().unwrap();

        assert!(matches!(exit_code, crate::ExitCode::Success));
        let migrated = fs::read_to_string(source_file).unwrap();
        assert!(migrated.contains("\"\\n    first\\n    second\\n\""));
        assert!(!migrated.contains("#\""));
    }

    #[test]
    fn hash_string_conversion_preserves_every_decoded_byte() {
        let raw = "##\"a\\b\"# c\n    indented\tline\r\0\u{0008}\u{000B}\u{000C}\"##";
        let quoted = hash_string_to_quoted(raw).expect("valid hash string");
        let decoded = baml_db::escape::unescape_string_literal(&quoted[1..quoted.len() - 1]);

        assert_eq!(
            decoded,
            "a\\b\"# c\n    indented\tline\r\0\u{0008}\u{000B}\u{000C}"
        );
    }

    #[test]
    fn migration_is_syntax_aware_and_handles_multiple_delimiters() {
        let source = r####"// #"comment"#
function legacy() -> string {
    let prefix = "élève";
    let untouched = `#"backtick text"#`;
    let first = #"line one
    line two"#;
    let second = ##"contains "# and a `tick`"##;
    first + second + untouched
}
"####;

        let migrated = migrate_removed_features(source).unwrap();

        assert!(migrated.starts_with("// #\"comment\"#\n"));
        assert!(migrated.contains("let prefix = \"élève\";"));
        assert!(migrated.contains("`#\"backtick text\"#`"));
        assert!(migrated.contains("\"line one\\n    line two\""));
        assert!(migrated.contains("\"contains \\\"# and a `tick`\""));
        assert_eq!(migrate_removed_features(&migrated).unwrap(), migrated);
    }

    #[test]
    fn migration_unblocks_formatter_without_backtick_dedent() {
        let source = "function legacy() -> string {\n    #\"\n    line one\n    line two\n\"#\n}\n";
        assert!(matches!(
            baml_fmt::format(source, &FormatOptions::default()),
            Err(baml_fmt::FormatterError::ParseErrors(_))
        ));

        let migrated = migrate_removed_features(source).unwrap();
        let formatted = baml_fmt::format(&migrated, &FormatOptions::default())
            .expect("migrated source should be valid and format normally");

        assert!(formatted.contains("\"\\n    line one\\n    line two\\n\""));
        assert!(!formatted.contains("#\""));
        assert!(!formatted.contains('`'));
    }

    #[test]
    fn migration_refuses_legacy_jinja_prompt_and_template_bodies() {
        let source = r###"function Greet(name: string) -> string {
    client: "openai/gpt-4o"
    prompt: #"Hello {{ name }}"#
}

template_string Legacy(name: string) #"Hello {{ name }}"#
"###;

        assert_eq!(
            migrate_removed_features(source),
            Err(RemovedFeatureMigrationError::LegacyJinjaTemplate)
        );
    }

    #[test]
    fn fix_removed_features_does_not_rewrite_legacy_jinja_prompts() {
        let tmp = tempfile::tempdir().unwrap();
        let source_file = tmp.path().join("legacy_prompt.baml");
        let source = r###"function Greet(name: string) -> string {
    client: "openai/gpt-4o"
    prompt: #"Hello {{ name }}"#
}
"###;
        fs::write(&source_file, source).unwrap();

        let args = FormatArgs {
            paths: vec![source_file.clone()],
            from: None,
            dry_run: false,
            fix_removed_features: true,
        };

        assert!(matches!(args.run().unwrap(), crate::ExitCode::Other));
        assert_eq!(fs::read_to_string(source_file).unwrap(), source);
    }

    #[test]
    fn fix_removed_features_honors_format_ignore() {
        let tmp = tempfile::tempdir().unwrap();
        let source_file = tmp.path().join("ignored.baml");
        let source = "// baml-format: ignore\nfunction ignored() -> string { #\"keep me\"# }\n";
        fs::write(&source_file, source).unwrap();

        let args = FormatArgs {
            paths: vec![source_file.clone()],
            from: None,
            dry_run: false,
            fix_removed_features: true,
        };

        assert!(matches!(args.run().unwrap(), crate::ExitCode::Success));
        assert_eq!(fs::read_to_string(source_file).unwrap(), source);
    }

    #[test]
    fn malformed_hash_string_is_not_rewritten() {
        let source = "function broken() -> string { #\"unclosed }\n";
        assert_eq!(migrate_removed_features(source).unwrap(), source);
    }

    #[test]
    fn malformed_jinja_hash_string_preserves_parser_diagnostics() {
        let source = "function broken() -> string {\n    prompt: #\"unclosed\n}\n";
        assert_eq!(migrate_removed_features(source).unwrap(), source);
    }

    #[test]
    fn default_discovery_walks_up_from_nested_dir() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"fmt-test\"\n",
        )
        .unwrap();
        let nested_dir = tmp.path().join("baml_src/nested");
        fs::create_dir_all(&nested_dir).unwrap();
        let main = tmp.path().join("baml_src/main.baml");
        fs::write(&main, "function main() -> string { \"hello\" }\n").unwrap();
        let main = fs::canonicalize(main).unwrap();

        let files = discover_project_files(Some(&nested_dir)).unwrap().unwrap();
        assert_eq!(files, vec![main]);
    }

    #[test]
    fn explicit_sibling_source_is_not_redirected_to_primary_baml_src() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"fmt-test\"\n",
        )
        .unwrap();
        let primary = tmp.path().join("baml_src");
        let alternate = tmp.path().join("baml_src_temp2");
        fs::create_dir(&primary).unwrap();
        fs::create_dir(&alternate).unwrap();
        fs::write(
            primary.join("primary.baml"),
            "function primary() -> int { 1 }\n",
        )
        .unwrap();
        let alternate_file = alternate.join("alternate.baml");
        fs::write(&alternate_file, "function alternate() -> int { 2 }\n").unwrap();

        let files = discover_project_files(Some(&alternate)).unwrap().unwrap();
        assert_eq!(files, vec![fs::canonicalize(alternate_file).unwrap()]);
    }
}
