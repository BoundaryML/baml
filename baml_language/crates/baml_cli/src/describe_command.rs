#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

use anyhow::{Context, Result};
use baml_db::baml_compiler2_hir;
use baml_lsp2_actions::{
    DefinitionKind, MatchAnnotation, ProjectSearchOptions, ResolvedTarget, SymbolDescription,
    TextMatch, describe, search_symbols, search_text,
};
use baml_project::ProjectDatabase;
use clap::Args;

use crate::{
    project_load::load_project_or_default,
    util::{line_number_at_offset, relative_path},
};

/// Parsed documentation entry for a BAML keyword topic.
#[derive(serde::Deserialize)]
struct BamlKeywordDoc {
    summary: String,
    #[serde(default)]
    syntax: Option<String>,
    #[serde(default)]
    details: Option<String>,
}

/// Parsed documentation entry for a TypeScript/JavaScript keyword topic.
#[derive(serde::Deserialize)]
struct TsKeywordDoc {
    message: String,
    #[serde(default)]
    see: Option<String>,
}

static BAML_KEYWORDS: LazyLock<HashMap<String, BamlKeywordDoc>> = LazyLock::new(|| {
    serde_yaml::from_str(baml_builtins2::BAML_KEYWORDS_YAML)
        .expect("failed to parse baml_keywords.yaml")
});

static TS_KEYWORDS: LazyLock<HashMap<String, TsKeywordDoc>> = LazyLock::new(|| {
    serde_yaml::from_str(baml_builtins2::TS_KEYWORDS_YAML)
        .expect("failed to parse ts_keywords.yaml")
});

/// Which question the output answers: `overview` builds the most useful
/// bounded summary (identity, signature, I/O shapes, usage), `source` prints
/// the implementation (the pre-view default behavior), `usage` ranks the best
/// examples of using the symbol, `impact` lists the full blast radius of a
/// change.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescribeView {
    Overview,
    Source,
    Usage,
    Impact,
}

#[derive(Args, Clone, Debug)]
pub struct DescribeArgs {
    /// Symbol names to describe
    #[arg(value_name = "SYMBOL")]
    pub names: Vec<String>,

    /// Discover symbols from literal OR queries, then describe them
    #[arg(long = "grep", value_name = "QUERY", action = clap::ArgAction::Append)]
    pub grep_queries: Vec<String>,

    /// List all symbols in the project
    #[arg(long)]
    pub symbols: bool,

    /// Filter discovered symbols by kind
    #[arg(long, value_delimiter = ',')]
    pub kind: Vec<String>,

    /// Filter discovered symbols and text matches by file path substring
    #[arg(long, value_name = "PATH")]
    pub file: Vec<String>,

    /// Maximum number of grep-discovered symbols
    #[arg(long, default_value_t = 12)]
    pub limit: usize,

    /// Match grep query text case-insensitively
    #[arg(short = 'i', long)]
    pub ignore_case: bool,

    /// Emit compact output intended for coding agents
    #[arg(long)]
    pub agent: bool,

    /// Project search starting point. Defaults to the current directory.
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,

    /// What to show: overview (default), source, usage, or impact
    #[arg(long, value_enum, default_value_t = DescribeView::Overview)]
    pub view: DescribeView,

    /// Maximum output lines (soft cap; default 30)
    #[arg(long, default_value_t = 30, value_name = "LINES")]
    pub max_lines: usize,

    /// Dependency expansion depth in overview: 0 = names only (default),
    /// 1 = direct shapes, 2+ = recurse into nested dependencies (cycle-safe)
    #[arg(long, default_value_t = 0)]
    pub depth: usize,

    /// Output results as JSON
    #[arg(long)]
    pub json: bool,
}

/// Find FQNs across the user and builtin packages that are fuzzy-similar to `name`.
///
/// Used to power "did you mean?" hints when a path doesn't resolve. Returns up
/// to `limit` candidates sorted by Jaro-Winkler similarity (descending).
pub fn suggest_similar(db: &ProjectDatabase, name: &str, limit: usize) -> Vec<String> {
    suggest_similar_kinded(db, name, limit)
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

/// Like [`suggest_similar`], but pairs each suggestion with its definition kind
/// (`None` for namespace/package paths) so callers can color the leaf by kind.
pub fn suggest_similar_kinded(
    db: &ProjectDatabase,
    name: &str,
    limit: usize,
) -> Vec<(String, Option<baml_lsp2_actions::DefinitionKind>)> {
    use baml_compiler2_hir::package::{PackageId, package_items};

    type Kind = Option<baml_lsp2_actions::DefinitionKind>;
    let mut all_paths: Vec<(String, Kind)> = Vec::new();

    // User package: items (kinded) + namespace dotted paths (no kind).
    let user_pkg = PackageId::new(db, baml_db::Name::new("user"));
    for entry in baml_lsp2_actions::list_package_items(db, user_pkg) {
        all_paths.push((entry.fqn(), Some(entry.kind)));
    }
    let user_pkg_items = package_items(db, user_pkg);
    for ns_path in user_pkg_items.namespaces.keys() {
        if !ns_path.is_empty() {
            all_paths.push((
                ns_path
                    .iter()
                    .map(baml_db::Name::as_str)
                    .collect::<Vec<_>>()
                    .join("."),
                None,
            ));
        }
    }

    // Builtin packages: bare package name + item paths + namespaces.
    for pkg_name in baml_lsp2_actions::non_user_package_names(db) {
        all_paths.push((pkg_name.clone(), None));
        let pkg = PackageId::new(db, baml_db::Name::new(&pkg_name));
        for entry in baml_lsp2_actions::list_package_items(db, pkg) {
            all_paths.push((entry.fqn(), Some(entry.kind)));
        }
        let pkg_info = package_items(db, pkg);
        for ns_path in pkg_info.namespaces.keys() {
            if !ns_path.is_empty() {
                let dotted = ns_path
                    .iter()
                    .map(baml_db::Name::as_str)
                    .collect::<Vec<_>>()
                    .join(".");
                all_paths.push((format!("{pkg_name}.{dotted}"), None));
            }
        }
    }

    // Match case-insensitively: agents shouldn't need to remember casing to
    // get a useful "did you mean?" hint. `strsim::jaro_winkler` is case-
    // sensitive, so we lowercase both sides before scoring.
    let needle_lower = name.to_ascii_lowercase();
    let mut scored: Vec<(f64, String, Kind)> = all_paths
        .into_iter()
        .map(|(p, kind)| {
            let p_lower = p.to_ascii_lowercase();
            // Jaro-Winkler on lowercased strings handles typos; substring
            // presence is an extra boost for cases like "Confg" → "Config".
            let mut score = strsim::jaro_winkler(&p_lower, &needle_lower);
            if p_lower.contains(&needle_lower) {
                score += 0.15;
            }
            (score, p, kind)
        })
        .filter(|(s, _, _)| *s > 0.7)
        .collect();

    // Sort by score desc, then alphabetically for stability.
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    // Dedup adjacent duplicates after sort.
    scored.dedup_by(|a, b| a.1 == b.1);
    scored
        .into_iter()
        .take(limit)
        .map(|(_, p, kind)| (p, kind))
        .collect()
}

/// Print a "Did you mean?" hint for `name` to stderr if any similar paths exist.
fn print_did_you_mean(db: &ProjectDatabase, name: &str) {
    let suggestions = suggest_similar_kinded(db, name, 5);
    if !suggestions.is_empty() {
        eprintln!();
        eprintln!("Did you mean:");
        // did-you-mean writes to stderr, so use a stderr-bound painter (gets the
        // stderr color decision, never stdout's).
        let painter = crate::paint::Painter::stderr();
        for (s, kind) in suggestions {
            eprintln!("  {}", painter.fqn_opt(&s, kind));
        }
    }
}

/// Dispatch a name string to a `ResolvedTarget`.
///
/// Handles the package-name prefix routing (user vs. builtin packages) and
/// delegates within-package path resolution to `baml_lsp2_actions::resolve_target`.
///
/// - Empty string → `Package(user)`
/// - `"baml"` → `Package(baml)`
/// - `"baml.env"` → `resolve_target(baml_pkg, "env")` → `Namespace`
/// - `"foo.bar.Baz"` → `resolve_target(user_pkg, "foo.bar.Baz")` → `Item`
pub fn dispatch<'db>(db: &'db ProjectDatabase, name: &str) -> Option<ResolvedTarget<'db>> {
    if name.is_empty() {
        let user_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"));
        return Some(ResolvedTarget::Package(user_pkg));
    }

    // Lowercase primitive/keyword aliases resolve to their builtin `baml`
    // companion class — `string` → `baml.String`, `image` → `baml.media.Image`,
    // `json` → `baml.json.json`. Also handles drilling into a member,
    // `string.length` → `baml.String.length`. Checked before the keyword
    // crosswalk so `baml describe string` shows the class (with its methods),
    // not keyword docs.
    let (alias_head, alias_rest) = name.split_once('.').unwrap_or((name, ""));
    if let Some(class_path) = builtin_alias_class_path(alias_head) {
        let baml_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("baml"));
        let target = if alias_rest.is_empty() {
            class_path.to_string()
        } else {
            format!("{class_path}.{alias_rest}")
        };
        return baml_lsp2_actions::resolve_target(db, baml_pkg, &target);
    }

    // Check for keyword (BAML or TS/JS crosswalk) before package routing.
    if BAML_KEYWORDS.contains_key(name) || TS_KEYWORDS.contains_key(name) {
        return Some(ResolvedTarget::Keyword(name.to_string()));
    }

    // Force user-package resolution with `root.` prefix.
    if let Some(rest) = name.strip_prefix("root.") {
        let user_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"));
        return baml_lsp2_actions::resolve_target(db, user_pkg, rest);
    }

    let (first, rest) = name.split_once('.').unwrap_or((name, ""));

    // Builtin package shadows user namespace with same name.
    let builtin_packages = baml_lsp2_actions::non_user_package_names(db);
    if builtin_packages.contains(first) {
        let pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new(first));
        return if rest.is_empty() {
            Some(ResolvedTarget::Package(pkg))
        } else {
            baml_lsp2_actions::resolve_target(db, pkg, rest)
        };
    }

    // User package.
    let user_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("user"));
    if let Some(target) = baml_lsp2_actions::resolve_target(db, user_pkg, name) {
        return Some(target);
    }

    resolve_unqualified_builtin_member(db, name)
}

/// Map a lowercase primitive/keyword alias to the path of its builtin `baml`
/// companion class, relative to the `baml` package. Mirrors the alias set in
/// `baml_compiler2_tir::ty::PrimitiveType::alias` plus the `json` type alias.
fn builtin_alias_class_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "string" => "String",
        "int" => "Int",
        "bigint" => "Bigint",
        "float" => "Float",
        "bool" => "Bool",
        "null" => "Null",
        "uint8array" => "Uint8Array",
        "image" => "media.Image",
        "audio" => "media.Audio",
        "video" => "media.Video",
        "pdf" => "media.Pdf",
        "json" => "json.json",
        _ => return None,
    })
}

/// Resolve `Array.reduce`/`String.split`-style builtin class member lookups.
///
/// Bare builtin class names are discoverable through the describe fallback, but
/// dotted member paths need a structural target before rendering can drill into
/// the method. This retry is intentionally after user-package lookup so a user
/// class named `Array` still owns `Array.foo`; `baml.Array.foo` remains the
/// explicit builtin spelling.
fn resolve_unqualified_builtin_member<'db>(
    db: &'db ProjectDatabase,
    name: &str,
) -> Option<ResolvedTarget<'db>> {
    let (class_name, _) = name.split_once('.')?;
    let baml_pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new("baml"));
    let baml_items = baml_compiler2_hir::package::package_items(db, baml_pkg);
    let root_ns: Vec<baml_db::Name> = Vec::new();
    let class_name = baml_db::Name::new(class_name);

    baml_items.lookup_type(&root_ns, &class_name)?;
    baml_lsp2_actions::resolve_target(db, baml_pkg, name)
}

impl DescribeArgs {
    fn describe_options(&self) -> baml_lsp2_actions::DescribeOptions {
        if self.json {
            return baml_lsp2_actions::DescribeOptions::default();
        }
        match self.view {
            DescribeView::Source => baml_lsp2_actions::DescribeOptions::source(),
            DescribeView::Usage | DescribeView::Impact => {
                baml_lsp2_actions::DescribeOptions::usage()
            }
            DescribeView::Overview => baml_lsp2_actions::DescribeOptions::overview(),
        }
    }

    /// Run the describe command and return the CLI exit code.
    pub fn run(&self) -> Result<crate::ExitCode> {
        if self.names.len() > 1 || !self.grep_queries.is_empty() || self.agent {
            return self.run_batch();
        }
        // Introspection never requires a `baml.toml`: with no project, we
        // fall back to a stdlib-only "default state" so `baml describe
        // baml.String` works anywhere. An empty user-file set is therefore
        // expected, not an error — unresolved names still surface through
        // the per-target "No symbol found" + did-you-mean paths below.
        let (db, from, _baml_files) = load_project_or_default(self.from.as_deref())?;

        // ── --symbols deprecation ───────────────────────────────────────────
        if self.symbols {
            eprintln!(
                "warning: --symbols is deprecated. Use `baml describe` with no arguments instead."
            );
        }

        let name = self.names.first().map(String::as_str).unwrap_or("");
        let target = dispatch(&db, name);

        match target {
            Some(ResolvedTarget::Keyword(ref kw)) => {
                if self.json {
                    let json = render_keyword_json(kw);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json)
                            .context("Failed to serialize keyword output as JSON")?
                    );
                } else {
                    render_keyword(kw);
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Package(pkg)) => {
                let entries = baml_lsp2_actions::list_package_items(&db, pkg);
                if entries.is_empty() {
                    eprintln!("No symbols found.");
                    return Ok(crate::ExitCode::Other);
                }
                if self.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                            .context("Failed to serialize output as JSON")?
                    );
                } else {
                    render_listing(&entries, &from);
                }
                // Check for name collision: does the user also have an item
                // matching the package name? Only hint when the resolved package
                // is a builtin (i.e., the bare name matches a non-user package).
                let pkg_name = name.split('.').next().unwrap_or(name);
                let builtin_names = baml_lsp2_actions::non_user_package_names(&db);
                if builtin_names.contains(pkg_name) {
                    let user_pkg = baml_compiler2_hir::package::PackageId::new(
                        &db,
                        baml_db::Name::new("user"),
                    );
                    if baml_lsp2_actions::resolve_target(&db, user_pkg, pkg_name).is_some() {
                        eprintln!();
                        eprintln!(
                            "Note: your project also defines `{pkg_name}`. \
                             Use `baml describe root.{pkg_name}` to see your definition."
                        );
                    }
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Namespace { package, ns_path }) => {
                let entries = baml_lsp2_actions::list_namespace_items(&db, package, &ns_path)
                    .unwrap_or_default();
                if entries.is_empty() {
                    eprintln!("No symbols found in namespace.");
                    return Ok(crate::ExitCode::Other);
                }
                if self.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                            .context("Failed to serialize output as JSON")?
                    );
                } else {
                    render_listing(&entries, &from);
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Item(def)) => {
                let describe_files = baml_compiler2_hir::compiler2_all_files(&db);
                if let Some(desc) = baml_lsp2_actions::describe_by_definition_with_options(
                    &db,
                    &describe_files,
                    def,
                    self.describe_options(),
                ) {
                    if self.json {
                        let json = description_to_json(&db, &desc, self.max_lines, &from);
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&[json])
                                .context("Failed to serialize output as JSON")?
                        );
                    } else {
                        render_view(
                            &db,
                            &describe_files,
                            &desc,
                            self.view,
                            self.max_lines,
                            self.depth,
                            &from,
                        );
                    }
                    Ok(crate::ExitCode::Success)
                } else {
                    eprintln!("No symbol found: {name}");
                    print_did_you_mean(&db, name);
                    Ok(crate::ExitCode::Other)
                }
            }
            Some(ResolvedTarget::Member {
                parent,
                member_name,
            }) => {
                let describe_files = baml_compiler2_hir::compiler2_all_files(&db);
                if let Some(desc) = baml_lsp2_actions::describe_item_member(
                    &db,
                    &describe_files,
                    parent,
                    member_name.as_str(),
                ) {
                    if self.json {
                        let json = description_to_json(&db, &desc, self.max_lines, &from);
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&[json])
                                .context("Failed to serialize output as JSON")?
                        );
                    } else {
                        render_view(
                            &db,
                            &describe_files,
                            &desc,
                            self.view,
                            self.max_lines,
                            self.depth,
                            &from,
                        );
                    }
                    Ok(crate::ExitCode::Success)
                } else {
                    eprintln!("No symbol found: {name}");
                    print_did_you_mean(&db, name);
                    Ok(crate::ExitCode::Other)
                }
            }
            None => {
                // Substring fallback (existing behavior for unresolved names).
                let describe_files = baml_compiler2_hir::compiler2_all_files(&db);
                let descriptions = describe(&db, &describe_files, name);

                if descriptions.is_empty() {
                    eprintln!("No symbol found: {name}");
                    print_did_you_mean(&db, name);
                    return Ok(crate::ExitCode::Other);
                }

                if self.json {
                    let max_lines = self.max_lines;
                    let json_output: Vec<serde_json::Value> = descriptions
                        .iter()
                        .map(|d| description_to_json(&db, d, max_lines, &from))
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json_output)
                            .context("Failed to serialize output as JSON")?
                    );
                    return Ok(crate::ExitCode::Success);
                }

                for (i, desc) in descriptions.iter().enumerate() {
                    if i > 0 {
                        println!();
                        println!();
                    }
                    render_view(
                        &db,
                        &describe_files,
                        desc,
                        self.view,
                        self.max_lines,
                        self.depth,
                        &from,
                    );
                }

                Ok(crate::ExitCode::Success)
            }
        }
    }

    fn run_batch(&self) -> Result<crate::ExitCode> {
        let (db, from, _baml_files) = load_project_or_default(self.from.as_deref())?;
        let files = baml_compiler2_hir::compiler2_all_files(&db);
        // Discovery is project-scoped. Builtins remain available when named
        // explicitly, but should not crowd user symbols out of a bounded
        // `--grep` result (for example, a user query for "outcome" should not
        // spend its first slot on testing.Outcome).
        let search_files = db.get_source_files();
        let kind_filter = parse_kind_filter(&self.kind)?;
        let terms = parse_grep_terms(&self.grep_queries)?;

        let mut requested = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in &self.names {
            if seen.insert(name.clone()) {
                requested.push(name.clone());
            }
        }

        let mut unmatched = Vec::new();
        let mut candidates_by_term = Vec::new();
        for term in &terms {
            let mut candidates = search_symbols(&db, &search_files, term)
                .into_iter()
                .filter(|symbol| symbol.container_name.is_none())
                .filter(|symbol| kind_filter.is_empty() || kind_filter.contains(&symbol.kind))
                .filter(|symbol| {
                    path_matches(&symbol.file.path(&db).display().to_string(), &self.file)
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|symbol| {
                let exact = symbol.name == *term;
                let case_exact = symbol.name.eq_ignore_ascii_case(term);
                (!exact, !case_exact, symbol.name.len(), symbol.name.clone())
            });

            let mut names = candidates
                .into_iter()
                .map(|candidate| candidate.name)
                .collect::<Vec<_>>();
            if names.is_empty() {
                let opts = ProjectSearchOptions {
                    pattern: term,
                    ignore_case: self.ignore_case,
                    kind_filter: &kind_filter,
                };
                let text_matches = search_text(&db, &search_files, &opts);
                for text_match in text_matches {
                    if !path_matches(&text_match.file_path, &self.file) {
                        continue;
                    }
                    let target = match &text_match.annotation {
                        Some(MatchAnnotation::Definition { name, .. }) => Some(name.clone()),
                        Some(MatchAnnotation::Reference { target_name, .. }) => {
                            Some(target_name.clone())
                        }
                        _ => None,
                    };
                    if let Some(target) = target {
                        if !names.contains(&target) {
                            names.push(target);
                        }
                    } else {
                        unmatched.push(text_match);
                    }
                }
            }
            candidates_by_term.push(names);
        }

        let mut discovered = 0usize;
        let mut candidate_index = 0usize;
        while discovered < self.limit {
            let mut advanced = false;
            for candidates in &candidates_by_term {
                if let Some(candidate) = candidates.get(candidate_index) {
                    advanced = true;
                    if seen.insert(candidate.clone()) {
                        requested.push(candidate.clone());
                        discovered += 1;
                        if discovered >= self.limit {
                            break;
                        }
                    }
                }
            }
            if !advanced {
                break;
            }
            candidate_index += 1;
        }

        let mut descriptions = Vec::new();
        let mut misses = Vec::new();
        for name in &requested {
            let target = dispatch(&db, name);
            let description = match target {
                Some(ResolvedTarget::Item(def)) => {
                    baml_lsp2_actions::describe_by_definition_with_options(
                        &db,
                        &files,
                        def,
                        self.describe_options(),
                    )
                }
                Some(ResolvedTarget::Member {
                    parent,
                    member_name,
                }) => baml_lsp2_actions::describe_item_member(
                    &db,
                    &files,
                    parent,
                    member_name.as_str(),
                ),
                _ => describe(&db, &files, name).into_iter().next(),
            };
            if let Some(description) = description {
                descriptions.push(description);
            } else {
                misses.push(name.clone());
            }
        }

        if descriptions.is_empty() {
            for miss in &misses {
                eprintln!("No symbol found: {miss}");
            }
            return Ok(crate::ExitCode::Other);
        }

        if self.json {
            let per_result_max_lines = self
                .max_lines
                .saturating_sub(descriptions.len())
                .checked_div(descriptions.len())
                .unwrap_or(0);
            let results = descriptions
                .iter()
                .map(|description| {
                    batch_description_to_json(
                        &db,
                        description,
                        self.view,
                        per_result_max_lines,
                        &from,
                    )
                })
                .collect::<Vec<_>>();
            let unmatched_json = unmatched
                .iter()
                .map(|text_match| text_match_to_json(&db, text_match, &from))
                .collect::<Vec<_>>();
            let next = misses
                .iter()
                .map(|name| format!("baml describe {name}"))
                .collect::<Vec<_>>();
            let envelope = serde_json::json!({
                "schema_version": 1,
                "query": { "symbols": self.names, "grep": terms },
                "results": results,
                "unmatched": unmatched_json,
                "omitted": misses,
                "next": next,
            });
            println!(
                "{}",
                serde_json::to_string(&envelope)
                    .context("Failed to serialize batch describe output")?
            );
        } else {
            write_batch_output(
                &mut std::io::stdout(),
                &db,
                &files,
                &descriptions,
                &unmatched,
                &misses,
                self,
                &from,
            )?;
        }

        Ok(crate::ExitCode::Success)
    }
}

pub(crate) fn parse_grep_terms(queries: &[String]) -> Result<Vec<String>> {
    let mut terms = Vec::new();
    for query in queries {
        for term in query.split("||") {
            let term = term.trim();
            if term.is_empty() {
                anyhow::bail!("Invalid --grep query: empty term around `||`");
            }
            terms.push(term.to_string());
        }
    }
    Ok(terms)
}

pub(crate) fn parse_kind_filter(kinds: &[String]) -> Result<Vec<DefinitionKind>> {
    kinds
        .iter()
        .map(|kind| match kind.as_str() {
            "class" => Ok(DefinitionKind::Class),
            "enum" => Ok(DefinitionKind::Enum),
            "function" => Ok(DefinitionKind::Function),
            "test" => Ok(DefinitionKind::Test),
            "client" => Ok(DefinitionKind::Client),
            "type_alias" => Ok(DefinitionKind::TypeAlias),
            "template_string" => Ok(DefinitionKind::TemplateString),
            "retry_policy" => Ok(DefinitionKind::RetryPolicy),
            "let" => Ok(DefinitionKind::Let),
            "field" => Ok(DefinitionKind::Field),
            "variant" => Ok(DefinitionKind::Variant),
            other => anyhow::bail!(
                "Unknown kind: {other}. Valid kinds: class, enum, function, test, client, type_alias, template_string, retry_policy, let, field, variant"
            ),
        })
        .collect()
}

pub(crate) fn path_matches(path: &str, filters: &[String]) -> bool {
    filters.is_empty() || filters.iter().any(|filter| path.contains(filter))
}

fn budget_body(desc: &SymbolDescription, budget: usize) -> String {
    let body_lines = desc.full_body.lines().collect::<Vec<_>>();
    if body_lines.len() <= budget {
        desc.full_body.clone()
    } else if budget >= 5 {
        truncate_body(&body_lines, budget).join("\n")
    } else {
        shape_with_elision(&desc.shape, &desc.full_body)
    }
}

fn text_match_to_json(
    db: &ProjectDatabase,
    text_match: &TextMatch,
    project_root: &std::path::Path,
) -> serde_json::Value {
    let file_path = relative_path(&text_match.file.path(db), project_root);
    let annotation = match &text_match.annotation {
        Some(MatchAnnotation::Definition { name, kind }) => serde_json::json!({
            "type": "definition", "name": name, "kind": kind.as_str(),
        }),
        Some(MatchAnnotation::Reference {
            target_name,
            target_kind,
        }) => serde_json::json!({
            "type": "reference", "target_name": target_name, "target_kind": target_kind.as_str(),
        }),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "file": file_path.to_string_lossy(),
        "line": text_match.line_number,
        "text": text_match.line_text.trim(),
        "annotation": annotation,
    })
}

fn method_json(
    db: &ProjectDatabase,
    project_root: &std::path::Path,
    methods: &[describe::MethodRef],
) -> Vec<serde_json::Value> {
    methods
        .iter()
        .map(|method| {
            let file_path = relative_path(&method.file.path(db), project_root);
            let text = method.file.text(db);
            serde_json::json!({
                "name": method.name,
                "signature": method.signature,
                "docstring": method.docstring,
                "file": file_path.to_string_lossy(),
                "line_start": line_number_at_offset(text, method.item_range.start().into()),
                "line_end": line_number_at_offset(text, method.item_range.end().into()),
            })
        })
        .collect()
}

pub fn description_to_json(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> serde_json::Value {
    let file_path = relative_path(&desc.file.path(db), project_root);
    serde_json::json!({
        "name": desc.name,
        "kind": desc.kind.as_str(),
        "file": file_path.to_string_lossy(),
        "line": line_number_at_offset(desc.file.text(db), desc.name_span.start().into()),
        "shape": desc.shape,
        "body": budget_body(desc, budget),
        "docstring": desc.docstring,
        "resolved_type": desc.resolved_type,
        "dependencies": desc.dependencies.iter().map(|dependency| {
            let path = relative_path(&dependency.file.path(db), project_root);
            serde_json::json!({
                "name": dependency.name, "kind": dependency.kind.as_str(),
                "file": path.to_string_lossy(),
                "line": line_number_at_offset(dependency.file.text(db), dependency.name_span.start().into()),
            })
        }).collect::<Vec<_>>(),
        "references": desc.references.iter().map(|reference| {
            let path = relative_path(&reference.file.path(db), project_root);
            serde_json::json!({
                "file": path.to_string_lossy(), "line": reference.line_number,
                "text": reference.line_text.trim(),
            })
        }).collect::<Vec<_>>(),
        "instance_methods": method_json(db, project_root, &desc.instance_methods),
        "static_methods": method_json(db, project_root, &desc.static_methods),
        "container": desc.container.as_ref().map(|container| {
            let path = relative_path(&container.file.path(db), project_root);
            serde_json::json!({
                "name": container.name, "kind": container.kind.as_str(),
                "file": path.to_string_lossy(),
                "line": line_number_at_offset(container.file.text(db), container.name_span.start().into()),
            })
        }),
    })
}

fn batch_description_to_json(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    view: DescribeView,
    budget: usize,
    project_root: &std::path::Path,
) -> serde_json::Value {
    let file_path = relative_path(&desc.file.path(db), project_root);
    let identity = serde_json::json!({
        "name": desc.name,
        "kind": desc.kind.as_str(),
        "file": file_path.to_string_lossy(),
        "line": line_number_at_offset(desc.file.text(db), desc.name_span.start().into()),
    });
    match view {
        DescribeView::Source => serde_json::json!({
            "identity": identity,
            "source": budget_body(desc, budget),
        }),
        DescribeView::Overview => serde_json::json!({
            "identity": identity,
            "shape": desc.shape,
            "docstring": desc.docstring,
            "dependencies": desc.dependencies.iter().take(budget).map(|dependency| dependency.name.as_str()).collect::<Vec<_>>(),
        }),
        DescribeView::Usage => serde_json::json!({
            "identity": identity,
            "references": desc.references.iter().take(budget).map(|reference| {
                let path = relative_path(&reference.file.path(db), project_root);
                serde_json::json!({"file": path.to_string_lossy(), "line": reference.line_number, "text": reference.line_text.trim()})
            }).collect::<Vec<_>>(),
            "omitted": desc.references.len().saturating_sub(budget),
        }),
        DescribeView::Impact => serde_json::json!({
            "identity": identity,
            "relationships": desc.references.iter().take(budget).map(|reference| {
                let path = relative_path(&reference.file.path(db), project_root);
                serde_json::json!({"file": path.to_string_lossy(), "line": reference.line_number, "text": reference.line_text.trim()})
            }).collect::<Vec<_>>(),
            "omitted": desc.references.len().saturating_sub(budget),
        }),
    }
}

pub(crate) fn write_batch_output(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    files: &[baml_db::SourceFile],
    descriptions: &[SymbolDescription],
    unmatched: &[TextMatch],
    misses: &[String],
    args: &DescribeArgs,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let output_max_lines = if args.agent {
        args.max_lines.min(80)
    } else {
        args.max_lines
    };
    let mut blocks = Vec::new();
    for description in descriptions {
        let mut bytes = Vec::new();
        match args.view {
            DescribeView::Source => {
                write_description(&mut bytes, db, description, output_max_lines, project_root)?;
            }
            DescribeView::Overview => {
                write_overview(
                    &mut bytes,
                    db,
                    files,
                    description,
                    output_max_lines,
                    args.depth,
                    project_root,
                )?;
            }
            DescribeView::Usage => {
                write_usage_view(&mut bytes, db, description, output_max_lines, project_root)?;
            }
            DescribeView::Impact => {
                write_impact_view(&mut bytes, db, description, output_max_lines, project_root)?;
            }
        }
        let rendered = String::from_utf8_lossy(&bytes)
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let mut block = Vec::new();
        if let Some(identity) = rendered.first() {
            block.push(identity.clone());
        }
        if args.agent {
            let mut seen_relationships = std::collections::HashSet::new();
            for dependency in description.dependencies.iter().take(6) {
                let path = relative_path(&dependency.file.path(db), project_root);
                let line = line_number_at_offset(
                    dependency.file.text(db),
                    dependency.name_span.start().into(),
                );
                let relationship = format!(
                    "depends on {} {}  {}:{}",
                    dependency.kind.as_str(),
                    dependency.name,
                    path.display(),
                    line
                );
                if seen_relationships.insert(relationship.clone()) {
                    block.push(relationship);
                }
            }
            for reference in description.references.iter().take(3) {
                let path = relative_path(&reference.file.path(db), project_root);
                let relationship = format!(
                    "used at {}:{}  {}",
                    path.display(),
                    reference.line_number,
                    reference.line_text.trim()
                );
                if seen_relationships.insert(relationship.clone()) {
                    block.push(relationship);
                }
            }
        }
        block.extend(rendered.into_iter().skip(1));
        blocks.push(block);
    }

    let minimum = blocks.len();
    let mut remaining = output_max_lines.saturating_sub(minimum);
    for block in &blocks {
        if let Some(identity) = block.first() {
            writeln!(w, "{identity}")?;
        }
    }

    let view = match args.view {
        DescribeView::Overview => "overview",
        DescribeView::Source => "source",
        DescribeView::Usage => "usage",
        DescribeView::Impact => "impact",
    };
    if args.agent {
        // Only reserve a follow-up hint when the global budget cannot give
        // every symbol even one content line. Partially bounded source is
        // already useful; advertising an executable `next:` command for it
        // caused agents to repeat the whole batch unnecessarily.
        let reserve_next = usize::from(remaining > 0 && remaining < blocks.len());
        let content_budget = remaining.saturating_sub(reserve_next);
        let mut emitted = vec![0usize; blocks.len()];
        let mut content_remaining = content_budget;
        for (index, block) in blocks.iter().enumerate() {
            let symbols_left = blocks.len() - index;
            let allowance = content_remaining.div_ceil(symbols_left);
            let shown = block.len().saturating_sub(1).min(allowance);
            for line in block.iter().skip(1).take(shown) {
                writeln!(w, "{line}")?;
            }
            emitted[index] = shown;
            content_remaining = content_remaining.saturating_sub(shown);
        }
        remaining = remaining.saturating_sub(emitted.iter().sum::<usize>());
        let omitted_names = blocks
            .iter()
            .enumerate()
            .filter(|(index, block)| block.len() > 1 && emitted[*index] == 0)
            .map(|(index, _)| descriptions[index].name.as_str())
            .collect::<Vec<_>>();
        if !omitted_names.is_empty() && remaining > 0 {
            writeln!(
                w,
                "next: baml describe {} --view {view} --max-lines 80 --agent",
                omitted_names.join(" ")
            )?;
            remaining -= 1;
        }
    } else {
        for (index, block) in blocks.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let content = block.len().saturating_sub(1);
            let reserve_hint = usize::from(content > remaining);
            let shown = content.min(remaining.saturating_sub(reserve_hint));
            for line in block.iter().skip(1).take(shown) {
                writeln!(w, "{line}")?;
            }
            remaining = remaining.saturating_sub(shown);
            let omitted = content.saturating_sub(shown);
            if omitted > 0 && remaining > 0 {
                writeln!(
                    w,
                    "… {omitted} lines omitted — baml describe {} --view {view} --max-lines {}",
                    descriptions[index].name,
                    block.len(),
                )?;
                remaining -= 1;
            }
        }
    }

    let mut seen_unmatched = std::collections::HashSet::new();
    for text_match in unmatched {
        if remaining == 0 {
            break;
        }
        let path = relative_path(&text_match.file.path(db), project_root);
        let line = format!(
            "unmatched {}:{}  {}",
            path.display(),
            text_match.line_number,
            text_match.line_text.trim()
        );
        if seen_unmatched.insert(line.clone()) {
            writeln!(w, "{line}")?;
            remaining -= 1;
        }
    }
    for miss in misses {
        if remaining == 0 {
            break;
        }
        writeln!(w, "not found: {miss} — baml describe --grep {miss}")?;
        remaining -= 1;
    }
    Ok(())
}

/// Render keyword documentation to a writer.
pub fn write_keyword(w: &mut impl std::io::Write, name: &str) -> std::io::Result<()> {
    let painter = crate::paint::Painter::stdout();
    if let Some(doc) = BAML_KEYWORDS.get(name) {
        writeln!(w, "{} — {}", painter.keyword(name), doc.summary)?;
        if let Some(ref syntax) = doc.syntax {
            writeln!(w)?;
            writeln!(w, "Syntax:")?;
            for line in syntax.trim_end().lines() {
                writeln!(w, "  {}", painter.fragment(line))?;
            }
        }
        if let Some(ref details) = doc.details {
            writeln!(w)?;
            writeln!(w, "{details}")?;
        }
    } else if let Some(doc) = TS_KEYWORDS.get(name) {
        writeln!(w, "{} — {}", painter.keyword(name), doc.message)?;
        if let Some(ref see) = doc.see {
            writeln!(w)?;
            writeln!(w, "See: baml describe {}", painter.fragment(see))?;
        }
    }
    Ok(())
}

/// Render keyword documentation to stdout.
fn render_keyword(name: &str) {
    let _ = write_keyword(&mut std::io::stdout(), name);
}

/// Render keyword documentation as JSON.
fn render_keyword_json(name: &str) -> serde_json::Value {
    if let Some(doc) = BAML_KEYWORDS.get(name) {
        serde_json::json!({
            "type": "keyword",
            "name": name,
            "summary": doc.summary,
            "syntax": doc.syntax,
            "details": doc.details,
        })
    } else if let Some(doc) = TS_KEYWORDS.get(name) {
        serde_json::json!({
            "type": "crosswalk",
            "name": name,
            "message": doc.message,
            "see": doc.see,
        })
    } else {
        serde_json::json!(null)
    }
}

/// Render a SymbolDescription to stdout with budget-based output.
pub fn render_description(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) {
    let _ = write_description(&mut std::io::stdout(), db, desc, budget, project_root);
}

/// Render a SymbolDescription to a writer with budget-based output.
pub fn write_description(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let file_path = desc.file.path(db);
    let file_text = desc.file.text(db);
    let (start_line, end_line) = definition_line_range(
        file_text,
        desc.item_range.start().into(),
        desc.item_range.end().into(),
    );

    // ── Header: kind name  (canonical-fqn)  file:start-end ──────────────────
    // The canonical FQN appears in parentheses only when it differs from the
    // bare name (a builtin alias like `string`, or a namespaced/dependency type
    // like `root.ns.Foo`).
    let kind_str = desc.kind.as_str();
    let rel_path = relative_path(&file_path, project_root);
    let painter = crate::paint::Painter::stdout();
    let colors = painter.enabled();
    let hl = crate::paint::Highlighter::new(db);
    let fqn_part = desc
        .canonical_fqn
        .as_deref()
        .map(|f| format!("  ({})", painter.fqn(f, desc.kind)))
        .unwrap_or_default();
    let name_display = painter.fqn(&desc.name, desc.kind);
    let loc = painter.location(
        &file_path,
        &rel_path.display().to_string(),
        &format!("{start_line}-{end_line}"),
    );

    writeln!(
        w,
        "{} {name_display}{fqn_part}  {loc}",
        painter.keyword(kind_str)
    )?;

    let mut lines_used = 1;

    // ── Body ─────────────────────────────────────────────────────────────────
    // The body slice already includes any leading `///` doc-comments
    // (the CST parser attaches them as the first children of the item
    // node, so they fall inside `node.text_range()` and round-trip
    // through `slice_text`). Rendering `desc.docstring` separately
    // would just duplicate the same lines, so leave the docstring
    // surfacing to JSON consumers and let the body show it once.
    let is_local = matches!(
        desc.kind,
        baml_lsp2_actions::DefinitionKind::Parameter | baml_lsp2_actions::DefinitionKind::Binding
    );
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if !is_local {
        // The separator blank line is deliberately not counted against the
        // budget: the body's available-line computation predates the section
        // budgeting below and is a documented guarantee (a fields-only class
        // body must fit at budget 5).
        writeln!(w)?;
        let available_for_body = budget.saturating_sub(lines_used);

        // Colored TTY output renders the verbatim definition slice through the
        // compiler's semantic tokens. Plain output (pipes, JSON, tests) keeps
        // the existing cleaned/truncated behavior byte-for-byte.
        if colors {
            lines_used += write_highlighted_body(w, &hl, desc, available_for_body)?;
        } else {
            let was_truncated = body_lines.len() > available_for_body;
            let shown_body_lines;
            if body_lines.len() <= available_for_body {
                for line in &body_lines {
                    writeln!(w, "{line}")?;
                }
                shown_body_lines = body_lines.len();
                lines_used += shown_body_lines;
            } else if available_for_body >= 5 {
                let truncated = truncate_body(&body_lines, available_for_body);
                for line in &truncated {
                    writeln!(w, "{line}")?;
                }
                shown_body_lines = truncated.len();
                lines_used += shown_body_lines;
            } else {
                let elided_text = shape_with_elision(&desc.shape, &desc.full_body);
                let elided_lines: Vec<&str> = elided_text.lines().collect();
                for line in &elided_lines {
                    writeln!(w, "{line}")?;
                }
                shown_body_lines = elided_lines.len();
                lines_used += shown_body_lines;
            }
            if was_truncated {
                writeln!(w)?;
                writeln!(
                    w,
                    "[INFO] Showing {shown} of {total} lines. Use --max-lines {needed} for full output.",
                    shown = shown_body_lines,
                    total = body_lines.len(),
                    needed = body_lines.len() + 1,
                )?;
                lines_used += 2;
            }
        }
    }

    // Whatever budget the body didn't consume flows into the list sections
    // below (methods, dependencies, references), in priority order. The
    // budget is soft: section headers are always emitted (so the symbol's
    // surface stays discoverable), entries are never split mid-unit, and
    // anything elided is replaced by an explicit "… <n> more lines" marker.
    let mut remaining = budget.saturating_sub(lines_used);

    // ── Methods (instance) ───────────────────────────────────────────────────
    remaining = write_method_section(
        w,
        db,
        &painter,
        project_root,
        "methods",
        &desc.instance_methods,
        remaining,
    )?;

    // ── Static methods ───────────────────────────────────────────────────────
    remaining = write_method_section(
        w,
        db,
        &painter,
        project_root,
        "static_methods",
        &desc.static_methods,
        remaining,
    )?;

    // ── Container ────────────────────────────────────────────────────────────
    // Always shown in full: it's a single entry and part of the symbol's
    // identity, like the header.
    if let Some(ref c) = desc.container {
        writeln!(w)?;
        writeln!(w, "container:")?;
        let c_abs = c.file.path(db);
        let c_path = relative_path(&c_abs, project_root);
        let c_line = line_number_at_offset(c.file.text(db), c.name_span.start().into());
        write_kind_row(
            w,
            &painter,
            c.kind,
            &c.name,
            &c_abs,
            &c_path.display().to_string(),
            c_line,
        )?;
        remaining = remaining.saturating_sub(3);
    }

    // ── Dependencies ─────────────────────────────────────────────────────────
    if !desc.dependencies.is_empty() {
        writeln!(w)?;
        writeln!(w, "dependencies:")?;
        remaining = remaining.saturating_sub(2);
        let mut elided = 0usize;
        for dep in &desc.dependencies {
            if remaining == 0 {
                elided += 1;
                continue;
            }
            let dep_abs = dep.file.path(db);
            let dep_path = relative_path(&dep_abs, project_root);
            let dep_line = line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
            write_kind_row(
                w,
                &painter,
                dep.kind,
                &dep.name,
                &dep_abs,
                &dep_path.display().to_string(),
                dep_line,
            )?;
            remaining -= 1;
        }
        write_elision_marker(w, elided)?;
    }

    // ── References ───────────────────────────────────────────────────────────
    // Lowest priority: references are the first thing to give way under a
    // tight budget. The header always shows the total count.
    writeln!(w)?;
    writeln!(w, "references ({}):", desc.references.len())?;
    remaining = remaining.saturating_sub(2);
    let mut elided = 0usize;
    for r in &desc.references {
        if remaining == 0 {
            elided += 1;
            continue;
        }
        let ref_abs = r.file.path(db);
        let ref_path = relative_path(&ref_abs, project_root);
        let preview = if colors {
            let h = hl.enclosing_line(r.file, r.range);
            if h.is_empty() {
                r.line_text.trim().to_string()
            } else {
                h
            }
        } else {
            r.line_text.trim().to_string()
        };
        let loc = painter.location(
            &ref_abs,
            &ref_path.display().to_string(),
            &r.line_number.to_string(),
        );
        writeln!(w, "  {loc}  {preview}")?;
        remaining -= 1;
    }
    write_elision_marker(w, elided)?;

    Ok(())
}

// ── Views ─────────────────────────────────────────────────────────────────────

/// Lines every overview must keep free for the guaranteed relationship tail
/// (blank + usage header + one representative reference + elision hint), so a
/// large contract or dependency graph can never starve it.
const USAGE_RESERVE: usize = 4;

/// Documented budget-overrun allowance for the budgeted views: the guaranteed
/// skeleton (identity line, section headers with counts, elision hints) always
/// renders, so output may exceed `--max-lines` by at most this many lines.
pub(crate) const MAX_BUDGET_OVERRUN: usize = 2;

/// Shared line accountant for all text views. Sections charge what they write
/// and consult `remaining()` before emitting variable-size content; guaranteed
/// skeleton lines are charged too, which is what bounds the overrun.
struct LineBudget {
    limit: usize,
    used: usize,
}

impl LineBudget {
    fn new(limit: usize) -> Self {
        Self { limit, used: 0 }
    }

    fn charge(&mut self, lines: usize) {
        self.used += lines;
    }

    fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.used)
    }
}

/// Dispatch a description to the renderer for the selected `--view`.
pub fn render_view(
    db: &ProjectDatabase,
    files: &[baml_db::SourceFile],
    desc: &SymbolDescription,
    view: DescribeView,
    budget: usize,
    depth: usize,
    project_root: &std::path::Path,
) {
    let w = &mut std::io::stdout();
    let _ = match view {
        DescribeView::Source => write_description(w, db, desc, budget, project_root),
        DescribeView::Overview => write_overview(w, db, files, desc, budget, depth, project_root),
        DescribeView::Usage => write_usage_view(w, db, desc, budget, project_root),
        DescribeView::Impact => write_impact_view(w, db, desc, budget, project_root),
    };
}

/// Render the `overview` view: the most useful bounded summary of a symbol.
///
/// Budget guarantees, in priority order: identity + signature, the
/// input/output contract (direct dependency shapes expanded one level),
/// relationship counts, at least one representative usage. The implementation
/// preview gets whatever budget remains. Every elision names the command that
/// expands it.
pub fn write_overview(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    files: &[baml_db::SourceFile],
    desc: &SymbolDescription,
    budget: usize,
    depth: usize,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    use baml_lsp2_actions::DefinitionKind as K;

    // Members and locals keep the classic rendering: they are already tiny.
    if desc.kind.is_member() || matches!(desc.kind, K::Parameter | K::Binding | K::Let) {
        return write_description(w, db, desc, budget, project_root);
    }

    let painter = crate::paint::Painter::stdout();
    let file_path = desc.file.path(db);
    let file_text = desc.file.text(db);
    let (start_line, end_line) = definition_line_range(
        file_text,
        desc.item_range.start().into(),
        desc.item_range.end().into(),
    );
    let rel_path = relative_path(&file_path, project_root);

    // ── identity: guaranteed ─────────────────────────────────────────────────
    let fqn_part = desc
        .canonical_fqn
        .as_deref()
        .map(|f| format!("  ({})", painter.fqn(f, desc.kind)))
        .unwrap_or_default();
    let loc = painter.location(
        &file_path,
        &rel_path.display().to_string(),
        &format!("{start_line}-{end_line}"),
    );
    writeln!(
        w,
        "{} {}{fqn_part}  {loc}",
        painter.keyword(desc.kind.as_str()),
        painter.fqn(&desc.name, desc.kind)
    )?;
    let mut b = LineBudget::new(budget);
    b.charge(1);
    if let Some(doc) = desc.docstring.as_deref().and_then(|d| d.lines().next()) {
        writeln!(w, "  {doc}")?;
        b.charge(1);
    }

    // One visited set for the whole overview: a dependency shape renders once
    // and stays name-only everywhere else, and recursive expansion at
    // `--depth 2+` terminates on cycles instead of re-expanding.
    let mut visited: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    visited.insert((desc.file.path(db).display().to_string(), desc.name.clone()));

    match desc.kind {
        K::Function | K::TemplateString => {
            write_function_overview(
                w,
                db,
                files,
                desc,
                depth,
                &mut b,
                &mut visited,
                project_root,
                &painter,
            )?;
            write_usage_summary(w, db, desc, project_root, &painter, &mut b)?;
            write_impl_preview(w, desc, b.remaining())?;
        }
        K::Class | K::Interface => {
            write_class_overview(
                w,
                db,
                files,
                desc,
                depth,
                &mut b,
                &mut visited,
                project_root,
                &painter,
            )?;
            write_usage_summary(w, db, desc, project_root, &painter, &mut b)?;
        }
        _ => {
            // enum, type alias, client, retry_policy, test: the (small) body
            // *is* the contract — show it whole when it fits, bounded when it
            // does not (a large enum must not blow the budget).
            writeln!(w)?;
            b.charge(1);
            let avail = b.remaining().saturating_sub(USAGE_RESERVE).max(4);
            let written = write_bounded_lines(w, &desc.full_body, avail, "", "--view source")?;
            b.charge(written);
            write_usage_summary(w, db, desc, project_root, &painter, &mut b)?;
        }
    }

    Ok(())
}

/// Signature, input/output contract, and execution metadata for a
/// function-like symbol. Charges every line to the shared budget.
#[allow(clippy::too_many_arguments)]
fn write_function_overview(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    files: &[baml_db::SourceFile],
    desc: &SymbolDescription,
    depth: usize,
    b: &mut LineBudget,
    visited: &mut std::collections::HashSet<(String, String)>,
    project_root: &std::path::Path,
    painter: &crate::paint::Painter,
) -> std::io::Result<()> {
    use baml_lsp2_actions::DefinitionKind as K;

    let sig = desc
        .resolved_type
        .as_deref()
        .map(|t| format!("{}{t}", desc.name))
        .unwrap_or_else(|| desc.shape.clone());
    writeln!(w)?;
    writeln!(w, "signature")?;
    writeln!(w, "  {}", painter.fragment(&sig))?;
    b.charge(3);

    // Input/output contract: expand each signature dependency's shape one
    // level, sided by where it appears relative to `->`. Inputs render before
    // outputs (the caller constructs inputs first), matching the priority
    // order in the spec.
    let ret = desc
        .resolved_type
        .as_deref()
        .and_then(|t| t.rsplit_once("->"))
        .map(|(_, r)| r.trim().to_string())
        .unwrap_or_default();
    let (inputs, outputs): (Vec<_>, Vec<_>) = desc
        .dependencies
        .iter()
        .filter(|dep| matches!(dep.kind, K::Class | K::Enum | K::TypeAlias))
        .partition(|dep| !type_mentions(&ret, &dep.name));
    for (side, deps) in [("input", inputs), ("output", outputs)] {
        for dep in deps {
            write_dep_node(
                w,
                db,
                files,
                dep,
                side,
                depth,
                0,
                b,
                visited,
                project_root,
                painter,
            )?;
        }
    }

    // Execution metadata. Client/throws/prompt are scanned from the body —
    // a rendering-level fallback until execution metadata is plumbed through
    // `SymbolDescription`.
    let client = extract_client(&desc.full_body);
    let throws = extract_throws(&desc.full_body, &desc.name);
    let prompt_lines = prompt_line_count(&desc.full_body);
    if client.is_some() || throws.is_some() || prompt_lines.is_some() {
        writeln!(w)?;
        writeln!(w, "execution")?;
        b.charge(2);
        if let Some(c) = &client {
            writeln!(w, "  client {}  — baml describe {c}", painter.fragment(c))?;
            b.charge(1);
        }
        if let Some(t) = &throws {
            writeln!(w, "  throws {t}")?;
            b.charge(1);
        }
        if let Some(n) = prompt_lines {
            writeln!(w, "  prompt {n} lines  — --view source")?;
            b.charge(1);
        }
    }
    Ok(())
}

/// Fields (the canonical shape — the contract), field types expanded one
/// level, and method signatures for a class or interface. Charges every line
/// to the shared budget; large field/method lists elide with a recovery
/// command instead of exceeding it.
#[allow(clippy::too_many_arguments)]
fn write_class_overview(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    files: &[baml_db::SourceFile],
    desc: &SymbolDescription,
    depth: usize,
    b: &mut LineBudget,
    visited: &mut std::collections::HashSet<(String, String)>,
    project_root: &std::path::Path,
    painter: &crate::paint::Painter,
) -> std::io::Result<()> {
    use baml_lsp2_actions::DefinitionKind as K;

    // The canonical fields-only shape is the contract for classes. For
    // interfaces the shape is a bare `interface X` line (method collection is
    // class-only upstream), so the contract is recovered from the body's
    // method signatures instead.
    if desc.shape.lines().count() > 1 {
        writeln!(w)?;
        b.charge(1);
        let avail = (b.limit * 2 / 5).clamp(4, 14);
        let written = write_bounded_lines(w, &desc.shape, avail, "", "--view source")?;
        b.charge(written);
    }

    if desc.kind == K::Interface {
        write_interface_signatures(w, &desc.full_body, b)?;
    }

    for dep in &desc.dependencies {
        if !matches!(dep.kind, K::Class | K::Enum | K::TypeAlias) {
            continue;
        }
        write_dep_node(
            w,
            db,
            files,
            dep,
            "type",
            depth,
            0,
            b,
            visited,
            project_root,
            painter,
        )?;
    }

    write_overview_methods(w, "methods", &desc.instance_methods, b)?;
    write_overview_methods(w, "static methods", &desc.static_methods, b)?;
    Ok(())
}

/// Method signatures scanned from an interface body: declarations without a
/// body block are the abstract surface implementors must supply; ones with a
/// body are default methods. Section headers with counts always render;
/// entries elide under budget pressure with a recovery command.
fn write_interface_signatures(
    w: &mut impl std::io::Write,
    body: &str,
    b: &mut LineBudget,
) -> std::io::Result<()> {
    let mut required: Vec<String> = Vec::new();
    let mut defaults: Vec<String> = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if !t.starts_with("function ") {
            continue;
        }
        if let Some(sig) = t.strip_suffix('{') {
            defaults.push(sig.trim_end().to_string());
        } else {
            required.push(t.to_string());
        }
    }
    for (label, sigs) in [("requires", required), ("default methods", defaults)] {
        if sigs.is_empty() {
            continue;
        }
        writeln!(w)?;
        writeln!(w, "{label} ({})", sigs.len())?;
        b.charge(2);
        let shown = sigs
            .len()
            .min(b.remaining().saturating_sub(USAGE_RESERVE).max(1));
        for sig in &sigs[..shown] {
            writeln!(w, "  {sig}")?;
            b.charge(1);
        }
        if shown < sigs.len() {
            writeln!(w, "  … {} more — --view source", sigs.len() - shown)?;
            b.charge(1);
        }
    }
    Ok(())
}

/// Compact one-line-per-method signature listing, budget-bounded.
fn write_overview_methods(
    w: &mut impl std::io::Write,
    label: &str,
    methods: &[baml_lsp2_actions::describe::MethodRef],
    b: &mut LineBudget,
) -> std::io::Result<()> {
    if methods.is_empty() {
        return Ok(());
    }
    writeln!(w)?;
    writeln!(w, "{label} ({})", methods.len())?;
    b.charge(2);
    let shown = methods
        .len()
        .min(b.remaining().saturating_sub(USAGE_RESERVE).max(1));
    for m in &methods[..shown] {
        writeln!(w, "  {}", m.signature)?;
        b.charge(1);
    }
    if shown < methods.len() {
        writeln!(w, "  … {} more — --view source", methods.len() - shown)?;
        b.charge(1);
    }
    Ok(())
}

/// Print up to `limit` lines of `text` under `indent`, keeping the final line
/// (usually the closing brace) and replacing the elided middle with a marker
/// naming the `hint` command that recovers it. Returns lines written.
fn write_bounded_lines(
    w: &mut impl std::io::Write,
    text: &str,
    limit: usize,
    indent: &str,
    hint: &str,
) -> std::io::Result<usize> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= limit {
        for line in &lines {
            writeln!(w, "{indent}{line}")?;
        }
        return Ok(lines.len());
    }
    if limit < 3 {
        writeln!(w, "{indent}{}", lines[0])?;
        writeln!(w, "{indent}… {} more lines — {hint}", lines.len() - 1)?;
        return Ok(2);
    }
    let head = limit - 2;
    for line in &lines[..head] {
        writeln!(w, "{indent}{line}")?;
    }
    writeln!(
        w,
        "{indent}… {} more lines — {hint}",
        lines.len() - head - 1
    )?;
    writeln!(w, "{indent}{}", lines[lines.len() - 1])?;
    Ok(head + 2)
}

/// Render one dependency of the described symbol.
///
/// The name line always renders — contract names stay discoverable at any
/// depth or budget. The shape renders when `depth ≥ 1` and budget allows;
/// `depth ≥ 2` recurses into the dependency's own type dependencies, indented
/// one level per hop. The shared `visited` set makes recursion cycle-safe and
/// keeps any shape from rendering twice in one output.
#[allow(clippy::too_many_arguments)]
fn write_dep_node(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    files: &[baml_db::SourceFile],
    dep: &baml_lsp2_actions::DepRef,
    label: &str,
    depth: usize,
    level: usize,
    b: &mut LineBudget,
    visited: &mut std::collections::HashSet<(String, String)>,
    project_root: &std::path::Path,
    painter: &crate::paint::Painter,
) -> std::io::Result<()> {
    use baml_lsp2_actions::DefinitionKind as K;
    const MAX_SHAPE_LINES: usize = 12;
    const MAX_NESTED_DEPS: usize = 4;

    let pad = "  ".repeat(level);
    let dep_abs = dep.file.path(db);
    if level == 0 {
        writeln!(w)?;
        b.charge(1);
    }
    if !visited.insert((dep_abs.display().to_string(), dep.name.clone())) {
        writeln!(
            w,
            "{pad}{label} {}  ↳ shown above; not re-expanded",
            painter.fqn(&dep.name, dep.kind)
        )?;
        b.charge(1);
        return Ok(());
    }
    let dep_rel = relative_path(&dep_abs, project_root);
    let dep_line = line_number_at_offset(dep.file.text(db), dep.name_span.start().into());
    let loc = painter.location(
        &dep_abs,
        &dep_rel.display().to_string(),
        &dep_line.to_string(),
    );
    writeln!(
        w,
        "{pad}{label} {}  {loc}",
        painter.fqn(&dep.name, dep.kind)
    )?;
    b.charge(1);
    if depth == 0 {
        return Ok(());
    }
    if b.remaining() <= USAGE_RESERVE {
        writeln!(w, "{pad}  … shape elided — baml describe {}", dep.name)?;
        b.charge(1);
        return Ok(());
    }
    let Some(dep_desc) = baml_lsp2_actions::describe_dependency(
        db,
        files,
        dep,
        baml_lsp2_actions::DescribeOptions::dependency_shape(),
    ) else {
        return Ok(());
    };
    let avail = MAX_SHAPE_LINES
        .min(b.remaining().saturating_sub(USAGE_RESERVE))
        .max(1);
    let hint = format!("baml describe {}", dep.name);
    let written = write_bounded_lines(w, &dep_desc.shape, avail, &format!("{pad}  "), &hint)?;
    b.charge(written);

    if depth > 1 {
        let nested: Vec<_> = dep_desc
            .dependencies
            .iter()
            .filter(|d| matches!(d.kind, K::Class | K::Enum | K::TypeAlias))
            .collect();
        for nd in nested.iter().take(MAX_NESTED_DEPS) {
            write_dep_node(
                w,
                db,
                files,
                nd,
                "dependency",
                depth - 1,
                level + 1,
                b,
                visited,
                project_root,
                painter,
            )?;
        }
        if nested.len() > MAX_NESTED_DEPS {
            writeln!(
                w,
                "{pad}  … {} more dependencies — baml describe {} --depth {}",
                nested.len() - MAX_NESTED_DEPS,
                dep.name,
                depth - 1
            )?;
            b.charge(1);
        }
    }
    Ok(())
}

/// Reference count plus the single most representative usage (ranked: tests,
/// then construction/call sites, preferring short self-contained lines).
fn write_usage_summary(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    project_root: &std::path::Path,
    painter: &crate::paint::Painter,
    b: &mut LineBudget,
) -> std::io::Result<()> {
    let total = desc.references.len();
    let tests = desc
        .references
        .iter()
        .filter(|r| is_test_ref(db, r, project_root))
        .count();
    writeln!(w)?;
    if tests > 0 {
        writeln!(
            w,
            "usage ({total} {}, {tests} in tests)",
            plural(total, "reference", "references")
        )?;
    } else {
        writeln!(
            w,
            "usage ({total} {})",
            plural(total, "reference", "references")
        )?;
    }
    b.charge(2);
    if let Some(r) = ranked_references(db, desc, project_root).first() {
        let abs = r.file.path(db);
        let rel = relative_path(&abs, project_root);
        let loc = painter.location(&abs, &rel.display().to_string(), &r.line_number.to_string());
        writeln!(w, "  {loc}  {}", ref_preview(r))?;
        b.charge(1);
        if total > 1 {
            writeln!(w, "  … {} more — --view usage", total - 1)?;
            b.charge(1);
        }
    }
    Ok(())
}

/// Bounded implementation preview from whatever budget the guaranteed
/// sections left over, with a pointer to the full source.
fn write_impl_preview(
    w: &mut impl std::io::Write,
    desc: &SymbolDescription,
    remaining: usize,
) -> std::io::Result<()> {
    let lines: Vec<&str> = desc.full_body.lines().collect();
    let total = lines.len();
    writeln!(w)?;
    if remaining < 4 {
        writeln!(w, "implementation — {total} lines — --view source")?;
        return Ok(());
    }
    writeln!(w, "implementation ({total} lines)")?;
    let shown = remaining.saturating_sub(2).min(total);
    for line in &lines[..shown] {
        writeln!(w, "  {line}")?;
    }
    if shown < total {
        writeln!(w, "  … {} more lines — --view source", total - shown)?;
    }
    Ok(())
}

/// Render the `usage` view: grouped (tests, then code) and ranked, showing
/// representative examples within `--max-lines`. Totals always render; omitted
/// references get a count and a budget value that recovers them.
pub(crate) fn write_usage_view(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let painter = crate::paint::Painter::stdout();
    let mut b = LineBudget::new(budget);
    let total = desc.references.len();
    writeln!(
        w,
        "usage of {}  ({total} {})",
        painter.fqn(&desc.name, desc.kind),
        plural(total, "reference", "references")
    )?;
    b.charge(1);
    let (tests, code): (Vec<_>, Vec<_>) = ranked_references(db, desc, project_root)
        .into_iter()
        .partition(|r| is_test_ref(db, r, project_root));
    let mut omitted = 0usize;
    for (label, refs) in [("tests", tests), ("code", code)] {
        if refs.is_empty() {
            continue;
        }
        // Group headers with counts always render: budget buys detail,
        // never visibility.
        writeln!(w)?;
        writeln!(w, "{label} ({})", refs.len())?;
        b.charge(2);
        for r in refs {
            if b.remaining() <= 1 {
                omitted += 1;
                continue;
            }
            let abs = r.file.path(db);
            let rel = relative_path(&abs, project_root);
            let loc =
                painter.location(&abs, &rel.display().to_string(), &r.line_number.to_string());
            writeln!(w, "  {loc}  {}", ref_preview(r))?;
            b.charge(1);
        }
    }
    if omitted > 0 {
        writeln!(
            w,
            "… {omitted} more references — re-run with --max-lines {}",
            total + 6
        )?;
    }
    Ok(())
}

/// Render the `impact` view: the blast radius of changing the symbol, grouped
/// by file (largest first). Total site/file counts always render; under
/// budget pressure the sample truncates with explicit omitted counts.
pub(crate) fn write_impact_view(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let painter = crate::paint::Painter::stdout();
    let mut b = LineBudget::new(budget);
    let mut by_file: Vec<(String, Vec<&baml_lsp2_actions::RefSite>)> = Vec::new();
    for r in &desc.references {
        match by_file.iter_mut().find(|(p, _)| *p == r.file_path) {
            Some((_, v)) => v.push(r),
            None => by_file.push((r.file_path.clone(), vec![r])),
        }
    }
    // Largest blast first; path order breaks ties for stable output.
    by_file.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let total_sites = desc.references.len();
    let total_files = by_file.len();
    writeln!(
        w,
        "impact of changing {}  ({total_sites} {} in {total_files} {})",
        painter.fqn(&desc.name, desc.kind),
        plural(total_sites, "site", "sites"),
        plural(total_files, "file", "files")
    )?;
    b.charge(1);
    let mut omitted_sites = 0usize;
    let mut omitted_files = 0usize;
    for (_, refs) in &by_file {
        if b.remaining() <= 2 {
            omitted_files += 1;
            omitted_sites += refs.len();
            continue;
        }
        let abs = refs[0].file.path(db);
        let rel = relative_path(&abs, project_root);
        writeln!(w)?;
        writeln!(w, "{} ({})", rel.display(), refs.len())?;
        b.charge(2);
        for r in refs {
            if b.remaining() <= 1 {
                omitted_sites += 1;
                continue;
            }
            let loc =
                painter.location(&abs, &rel.display().to_string(), &r.line_number.to_string());
            writeln!(w, "  {loc}  {}", ref_preview(r))?;
            b.charge(1);
        }
    }
    if omitted_files > 0 || omitted_sites > 0 {
        let needed = total_sites + 2 * total_files + 1;
        if omitted_files > 0 {
            writeln!(
                w,
                "… {omitted_files} more files, {omitted_sites} more sites — re-run with --max-lines {needed}"
            )?;
        } else {
            writeln!(
                w,
                "… {omitted_sites} more sites — re-run with --max-lines {needed}"
            )?;
        }
    }
    Ok(())
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// Heuristic: is this reference inside a test?
///
/// Matches "test" against the project-root-relative path — never the absolute
/// path, which would misclassify every reference in a project checked out
/// under a directory whose name happens to contain "test".
fn is_test_ref(
    db: &ProjectDatabase,
    r: &baml_lsp2_actions::RefSite,
    project_root: &std::path::Path,
) -> bool {
    let rel = relative_path(&r.file.path(db), project_root);
    rel.display().to_string().contains("test") || r.line_text.trim_start().starts_with("test")
}

/// One-line preview of a reference site, truncated so a single long source
/// line cannot dominate a budgeted view.
fn ref_preview(r: &baml_lsp2_actions::RefSite) -> String {
    let text = r.line_text.trim();
    if text.chars().count() > 100 {
        format!("{}…", text.chars().take(99).collect::<String>())
    } else {
        text.to_string()
    }
}

/// Rank references by how representative they are as usage examples: tests
/// first, then implementations and construction sites, then calls, preferring
/// short self-contained lines. Stable order (path, then line) breaks ties.
fn ranked_references<'a>(
    db: &ProjectDatabase,
    desc: &'a SymbolDescription,
    project_root: &std::path::Path,
) -> Vec<&'a baml_lsp2_actions::RefSite> {
    let score = |r: &baml_lsp2_actions::RefSite| -> i32 {
        let text = r.line_text.trim();
        let mut score = 0;
        if is_test_ref(db, r, project_root) {
            score += 100;
        }
        if text.contains(&format!("implements {}", desc.name)) {
            score += 90;
        }
        if text.contains(&format!("{} {{", desc.name)) {
            score += 80;
        }
        if text.contains('(') {
            score += 40;
        }
        if text.starts_with("function ") {
            score += 20;
        }
        score - i32::try_from(text.len().min(200)).unwrap_or(200) / 20
    };
    let mut refs = desc.references.iter().collect::<Vec<_>>();
    refs.sort_by(|a, b| {
        score(b)
            .cmp(&score(a))
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.line_number.cmp(&b.line_number))
    });
    refs
}

/// Does the rendered type expression mention `name` as a whole identifier?
fn type_mentions(expr: &str, name: &str) -> bool {
    expr.match_indices(name).any(|(i, _)| {
        let before = expr[..i].chars().next_back();
        let after = expr[i + name.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The `client` a function-like item runs on, scanned from its body
/// (`client: Sonnet,` or block-form `client Sonnet`).
fn extract_client(body: &str) -> Option<String> {
    for line in body.lines() {
        let t = line.trim();
        let rest = if let Some(r) = t.strip_prefix("client:") {
            r
        } else if let Some(r) = t.strip_prefix("client ") {
            r
        } else {
            continue;
        };
        let name: String = rest
            .trim()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

/// The `throws …` clause on the symbol's signature line, if any.
fn extract_throws(body: &str, name: &str) -> Option<String> {
    let sig = body
        .lines()
        .find(|l| l.contains("function") && l.contains(name))?;
    let (_, t) = sig.split_once(" throws ")?;
    Some(t.trim_end_matches('{').trim().to_string())
}

/// Line count of the `prompt #"…"#` block, if the body has one.
fn prompt_line_count(body: &str) -> Option<usize> {
    let mut in_prompt = false;
    let mut n = 0usize;
    for line in body.lines() {
        if !in_prompt {
            if line.trim_start().starts_with("prompt") && line.contains("#\"") {
                in_prompt = true;
                n = 1;
            }
        } else {
            n += 1;
            if line.contains("\"#") {
                return Some(n);
            }
        }
    }
    None
}

/// Render the definition body with ANSI highlighting (colored-output path).
///
/// Highlights the verbatim source slice of the item via the compiler's semantic
/// tokens, then applies the soft line budget by line count (so an ANSI escape
/// run is never split). Returns the number of output lines consumed.
fn write_highlighted_body(
    w: &mut impl std::io::Write,
    hl: &crate::paint::Highlighter,
    desc: &SymbolDescription,
    available_for_body: usize,
) -> std::io::Result<usize> {
    let colored = hl.range(desc.file, desc.item_range);
    let all: Vec<&str> = colored.lines().collect();
    // `item_range` can swallow leading doc-comments/blank lines and trailing
    // whitespace; trim blank edges so the block starts at the declaration.
    let first = all.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let last = all
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(first, |e| e + 1);
    let lines = &all[first..last];

    if lines.len() <= available_for_body {
        for line in lines {
            writeln!(w, "{line}")?;
        }
        return Ok(lines.len());
    }

    // Doesn't fit: show a head/tail window with a skip marker, split purely on
    // line count. One line is reserved for the marker; `saturating_sub` keeps a
    // tiny budget (0..=2) from underflowing or dumping the whole body, and
    // biasing `head` upward favors showing the declaration line.
    let window = available_for_body.saturating_sub(1);
    let head = window.div_ceil(2);
    let tail = window - head;
    for line in &lines[..head] {
        writeln!(w, "{line}")?;
    }
    writeln!(w, "  [... skipped {} lines ...]", lines.len() - head - tail)?;
    for line in &lines[lines.len() - tail..] {
        writeln!(w, "{line}")?;
    }
    writeln!(w)?;
    writeln!(
        w,
        "[INFO] Showing {} of {} lines. Use --max-lines {} for full output.",
        head + tail + 1,
        lines.len(),
        lines.len() + 1,
    )?;
    Ok(head + tail + 3)
}

/// Write the soft-budget elision marker for `elided` hidden lines (no-op when
/// nothing was elided).
fn write_elision_marker(w: &mut impl std::io::Write, elided: usize) -> std::io::Result<()> {
    if elided > 0 {
        writeln!(
            w,
            "  … {elided} more lines (re-run with a higher --max-lines)"
        )?;
    }
    Ok(())
}

/// The 1-based inclusive line range of a definition, given the trivia-inclusive
/// byte range of its CST/HIR node.
///
/// Node ranges swallow leading blank lines, `///` doc-comments and `//` line
/// comments, plus trailing whitespace up to the next sibling. This trims both
/// ends so the range covers the real declaration: the start is the first line
/// that is neither blank nor a comment (the `class`/`function`/… line), and the
/// end is the last line carrying non-whitespace (the closing brace).
pub(crate) fn definition_line_range(
    text: &str,
    start_off: usize,
    end_off: usize,
) -> (usize, usize) {
    let end_off = end_off.min(text.len());
    let span = text.get(start_off..end_off).unwrap_or("");

    // Forward to the first non-blank, non-comment line.
    let mut real_start = start_off;
    let mut cursor = start_off;
    for chunk in span.split_inclusive('\n') {
        let trimmed = chunk.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            cursor += chunk.len();
            real_start = cursor;
        } else {
            real_start = cursor;
            break;
        }
    }

    // Backward over trailing whitespace to the last content byte.
    let bytes = text.as_bytes();
    let mut real_end = end_off.saturating_sub(1);
    while real_end > real_start && bytes[real_end].is_ascii_whitespace() {
        real_end -= 1;
    }
    // A span with no content (empty or comment-only) would leave
    // `real_end < real_start`; clamp so the range never reverses.
    real_end = real_end.max(real_start);

    (
        line_number_at_offset(text, real_start),
        line_number_at_offset(text, real_end),
    )
}

/// Render a `methods:` / `static_methods:` section.
///
/// Each method shows its first-line docstring (when present) followed by its
/// canonical signature and full definition line range. The section consumes
/// from the soft line `budget` and returns what's left: the header is always
/// emitted, each method is an atomic unit (docstring + signature are never
/// split, even if the last one runs slightly over), and methods that don't
/// fit are summarized by an elision marker.
fn write_method_section(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    painter: &crate::paint::Painter,
    project_root: &std::path::Path,
    label: &str,
    methods: &[describe::MethodRef],
    budget: usize,
) -> std::io::Result<usize> {
    if methods.is_empty() {
        return Ok(budget);
    }
    writeln!(w)?;
    writeln!(w, "{label}:")?;
    let mut remaining = budget.saturating_sub(2);
    let mut elided_lines = 0usize;
    for m in methods {
        let unit_cost = 1 + usize::from(m.docstring.is_some());
        if remaining == 0 {
            elided_lines += unit_cost;
            continue;
        }
        if let Some(doc) = &m.docstring {
            // `fragment` renders `///` lines as comments (and self-gates on color).
            let doc_line = painter.fragment(&format!("/// {doc}"));
            writeln!(w, "  {doc_line}")?;
        }
        let text = m.file.text(db);
        let (start, end) =
            definition_line_range(text, m.item_range.start().into(), m.item_range.end().into());
        let m_abs = m.file.path(db);
        let m_path = relative_path(&m_abs, project_root);
        let loc = painter.location(
            &m_abs,
            &m_path.display().to_string(),
            &format!("{start}-{end}"),
        );
        let sig = painter.fragment(&m.signature);
        writeln!(w, "  {sig}  {loc}")?;
        remaining = remaining.saturating_sub(unit_cost);
    }
    write_elision_marker(w, elided_lines)?;
    Ok(remaining)
}

/// Render a flat listing of entries to stdout.
fn render_listing(entries: &[baml_lsp2_actions::ListingEntry], project_root: &std::path::Path) {
    let _ = write_listing(&mut std::io::stdout(), entries, project_root);
}

/// Render a flat listing of entries to a writer.
pub fn write_listing(
    w: &mut impl std::io::Write,
    entries: &[baml_lsp2_actions::ListingEntry],
    project_root: &std::path::Path,
) -> std::io::Result<()> {
    let painter = crate::paint::Painter::stdout();
    for entry in entries {
        let abs = std::path::Path::new(&entry.file_path);
        let rel = relative_path(abs, project_root);
        let loc = painter.location(abs, &rel.display().to_string(), &entry.line.to_string());
        write_symbol_row(w, &painter, "", entry.kind, &entry.fqn(), &loc)?;
    }
    Ok(())
}

/// Write one `kind  name  location` row, colored by kind. `indent` is prepended
/// verbatim and `location` is preformatted (e.g. `path:line`).
fn write_symbol_row(
    w: &mut impl std::io::Write,
    painter: &crate::paint::Painter,
    indent: &str,
    kind: baml_lsp2_actions::DefinitionKind,
    name: &str,
    location: &str,
) -> std::io::Result<()> {
    writeln!(
        w,
        "{indent}{} {} {location}",
        painter.kind_label(kind, 16),
        painter.name_padded(name, kind, 32),
    )
}

/// Write an indented dependency / container row, colored by kind.
fn write_kind_row(
    w: &mut impl std::io::Write,
    painter: &crate::paint::Painter,
    kind: baml_lsp2_actions::DefinitionKind,
    name: &str,
    abs_path: &std::path::Path,
    rel_display: &str,
    line: usize,
) -> std::io::Result<()> {
    let loc = painter.location(abs_path, rel_display, &line.to_string());
    write_symbol_row(w, painter, "  ", kind, name, &loc)
}

/// Convert listing entries to JSON array.
fn listing_to_json(
    db: &ProjectDatabase,
    entries: &[baml_lsp2_actions::ListingEntry],
    project_root: &std::path::Path,
) -> Vec<serde_json::Value> {
    let _ = db; // db not needed for listing JSON, but kept for consistency
    entries
        .iter()
        .map(|entry| {
            let rel = relative_path(std::path::Path::new(&entry.file_path), project_root);
            serde_json::json!({
                "kind": entry.kind.as_str(),
                "name": entry.fqn(),
                "file": rel.to_string_lossy(),
                "line": entry.line,
            })
        })
        .collect()
}

/// Find the 0-based line index within a body where a span starts.
/// `item_range` is the range of the whole body, `name_span` is the variable's span.
fn find_line_in_body(
    body: &str,
    item_range: text_size::TextRange,
    name_span: text_size::TextRange,
) -> usize {
    // Calculate offset of name_span within the body.
    let body_start: usize = item_range.start().into();
    let name_start: usize = name_span.start().into();
    let relative_offset = name_start.saturating_sub(body_start);

    // Count newlines before the relative offset.
    body.chars()
        .take(relative_offset)
        .filter(|&c| c == '\n')
        .count()
}

/// Render body lines with context around a target line, using truncation if needed.
/// Always shows the function header (first line), then context around the target.
/// Returns the number of lines printed.
fn render_body_with_context(
    body_lines: &[&str],
    target_line: usize,
    available_budget: usize,
) -> usize {
    let total_lines = body_lines.len();

    if total_lines == 0 {
        return 0;
    }

    // If everything fits, just print it all.
    if total_lines <= available_budget {
        for line in body_lines {
            println!("{line}");
        }
        return total_lines;
    }

    // Otherwise, always show header + context around target_line with truncation.
    // Reserve up to 2 lines for "... skipped N lines ..." markers.
    let mut lines_printed = 0;

    // Always print the function header (line 0).
    println!("{}", body_lines[0]);
    lines_printed += 1;

    if total_lines == 1 {
        return lines_printed;
    }

    // Calculate how much budget remains for context.
    let remaining_budget = available_budget.saturating_sub(lines_printed + 2); // reserve for skip markers
    if remaining_budget == 0 {
        if total_lines > 1 {
            println!("  [... skipped {} lines ...]", total_lines - 1);
            lines_printed += 1;
        }
        return lines_printed;
    }

    // If target is on line 0 (the header), show lines after it.
    if target_line == 0 {
        let end = (1 + remaining_budget).min(total_lines);
        for line in &body_lines[1..end] {
            println!("{line}");
            lines_printed += 1;
        }
        if end < total_lines {
            println!("  [... skipped {} lines ...]", total_lines - end);
            lines_printed += 1;
        }
        return lines_printed;
    }

    // Distribute context around target_line (excluding header which is already printed).
    let half = remaining_budget / 2;
    let context_start = target_line.saturating_sub(half).max(1); // don't re-print header
    let context_end = (target_line + half + 1).min(total_lines);

    // Print skip marker if there's a gap after the header.
    if context_start > 1 {
        println!("  [... skipped {} lines ...]", context_start - 1);
        lines_printed += 1;
    }

    // Print the context lines.
    for line in &body_lines[context_start..context_end] {
        println!("{line}");
        lines_printed += 1;
    }

    // Print skip marker if there's more after context.
    if context_end < total_lines {
        println!("  [... skipped {} lines ...]", total_lines - context_end);
        lines_printed += 1;
    }

    lines_printed
}

/// Truncate a function body to fit within a line budget while preserving key content.
///
/// When the budget is too tight for even truncated output, produce a shape
/// with `{ ... }` appended to indicate a body was elided.
///
/// If the full body contains a `{` block but the shape doesn't end with one,
/// appends ` { ... }`. Otherwise returns the shape as-is.
pub fn shape_with_elision(shape: &str, full_body: &str) -> String {
    let has_block = full_body.contains('{');
    let shape_already_has_block = shape.contains('{');
    if has_block && !shape_already_has_block {
        format!("{shape} {{ ... }}")
    } else {
        shape.to_string()
    }
}

/// Priority:
/// 1. Always show the function header (first line)
/// 2. Preserve `//# ...` annotated comment blocks and their following content
/// 3. Show first `head_lines` and last `tail_lines` of the body
/// 4. Insert "... skipped N lines ..." at the appropriate indentation
pub fn truncate_body(body_lines: &[&str], available_lines: usize) -> Vec<String> {
    if body_lines.len() <= available_lines {
        return body_lines.iter().map(|s| s.to_string()).collect();
    }

    // Find annotated comment blocks (lines starting with `//#` after trimming).
    let mut annotated_ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < body_lines.len() {
        if body_lines[i].trim().starts_with("//#") {
            let start = i;
            // Include the comment and following non-empty, non-comment lines
            // until we hit another //# or a blank line or end of body.
            i += 1;
            while i < body_lines.len() {
                let trimmed = body_lines[i].trim();
                if trimmed.is_empty() || trimmed.starts_with("//#") {
                    break;
                }
                i += 1;
            }
            annotated_ranges.push((start, i));
        } else {
            i += 1;
        }
    }

    // Calculate how many lines annotated blocks consume.
    let annotated_line_count: usize = annotated_ranges
        .iter()
        .map(|(start, end)| end - start)
        .sum();

    // Reserve lines for: header (1), skip messages (up to 2).
    let skip_line_reserve = 2;
    let header_reserve = 1;
    let remaining_for_content = available_lines
        .saturating_sub(header_reserve)
        .saturating_sub(skip_line_reserve)
        .saturating_sub(annotated_line_count);

    // Distribute remaining between head and tail.
    let head_lines_count = remaining_for_content / 2;
    let tail_lines_count = remaining_for_content.saturating_sub(head_lines_count);

    let mut included: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // Always include header (line 0).
    included.insert(0);

    // Include head lines (after header).
    for idx in 1..(1 + head_lines_count).min(body_lines.len()) {
        included.insert(idx);
    }

    // Include tail lines.
    let tail_start = body_lines.len().saturating_sub(tail_lines_count);
    for idx in tail_start..body_lines.len() {
        included.insert(idx);
    }

    // Include annotated ranges.
    for (start, end) in &annotated_ranges {
        for idx in *start..*end {
            included.insert(idx);
        }
    }

    // Build output with skip markers.
    let mut result = Vec::new();
    let mut last_included: Option<usize> = None;

    for (idx, line) in body_lines.iter().enumerate() {
        if included.contains(&idx) {
            // Check if we need a skip marker.
            if let Some(last) = last_included {
                let skipped = idx - last - 1;
                if skipped > 0 {
                    // Detect indentation from the current line.
                    let indent = line.len() - line.trim_start().len();
                    let indent_str: String = line.chars().take(indent).collect();
                    result.push(format!("{}[... skipped {} lines ...]", indent_str, skipped));
                }
            }
            result.push(line.to_string());
            last_included = Some(idx);
        }
    }

    result
}
