#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_db::{ProjectDatabase, baml_compiler2_hir};
use baml_ide::{ResolvedTarget, SymbolDescription, describe};
use clap::Args;

use crate::util::{line_number_at_offset, relative_path};

/// Describe BAML symbols and language concepts.
///
/// With no name, lists symbols in the current project. A name can identify a
/// project symbol, builtin package, namespace, type, member, or BAML keyword.
/// Builtin documentation works outside a project, so `baml describe baml` is
/// the entry point for exploring the complete standard library.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  List project symbols:
    baml describe

  Describe the standard library:
    baml describe baml

  Describe a namespace:
    baml describe baml.json

  Describe a class:
    baml describe Array

  Describe a method:
    baml describe String.split

  Describe a keyword:
    baml describe match

  Search for something by what it does:
    baml describe --search 'read a file'")]
pub struct DescribeArgs {
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Symbol, namespace, package, or keyword. Omit to list project symbols.
    pub name: Option<String>,

    /// Deprecated alias for invoking `baml describe` without a name.
    #[arg(long, hide_short_help = true)]
    pub symbols: bool,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    /// Soft maximum number of output lines.
    #[arg(long, default_value_t = 30, help_heading = "Output options")]
    pub budget: usize,

    /// Output results as JSON
    #[arg(long, help_heading = "Output options")]
    pub json: bool,

    /// Search names *and* docstrings for NAME, instead of resolving it.
    ///
    /// `describe` answers "what is this called"; `--search` answers "what does
    /// this", which is the question you have when you know the job and not the
    /// name: `baml describe --search 'read a file'`. Matching is on whole words
    /// — of names and of docstrings — best first, so it finds a symbol whose
    /// documentation uses your words even when its name does not.
    #[arg(long, help_heading = "Output options")]
    pub search: bool,

    /// Most results to return from `--search`. At least 1.
    ///
    /// Bounded below because `--limit 0` truncated a full result set to nothing
    /// and then reported "no symbol matches", which is a different answer.
    #[arg(
        long,
        default_value_t = 30,
        value_name = "N",
        value_parser = clap::value_parser!(u16).range(1..),
        help_heading = "Output options"
    )]
    pub limit: u16,

    /// Export a whole package's surface as one versioned JSON document
    /// (NAME must be a package: `baml`, `user`, …). Cross-package references
    /// are self-describing ids like `T:baml.time.Duration` — the prefix is
    /// the kind (`T:` type, `V:` value, `M:` method, `F:` field, `E:`
    /// variant, `A:` associated type; BAML's type and value namespaces are
    /// distinct, so bare paths would be ambiguous). Export the referenced
    /// package for a foreign id's full record. Filter with jq, e.g.
    /// `… --export | jq '.items[] | select(.namespace == ["json"])'`.
    #[arg(long, help_heading = "Output options")]
    pub export: bool,
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
) -> Vec<(String, Option<baml_ide::DefinitionKind>)> {
    use baml_compiler2_hir::package::{PackageId, package_items};

    type Kind = Option<baml_ide::DefinitionKind>;
    let mut all_paths: Vec<(String, Kind)> = Vec::new();

    // User package: items (kinded) + namespace dotted paths (no kind).
    let user_pkg = baml_compiler2_hir::package::sole_workspace_package(db);
    for entry in baml_ide::list_package_items(db, user_pkg) {
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
    for pkg_name in baml_ide::non_workspace_package_names(db) {
        all_paths.push((pkg_name.as_str().to_string(), None));
        let pkg = PackageId::new(db, pkg_name.clone());
        for entry in baml_ide::list_package_items(db, pkg) {
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
                all_paths.push((format!("{}.{dotted}", pkg_name.as_str()), None));
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
        eprintln!("did you mean:");
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
/// delegates within-package path resolution to `baml_ide::resolve_target`.
///
/// - Empty string → `Package(user)`
/// - `"baml"` → `Package(baml)`
/// - `"baml.env"` → `resolve_target(baml_pkg, "env")` → `Namespace`
/// - `"foo.bar.Baz"` → `resolve_target(user_pkg, "foo.bar.Baz")` → `Item`
pub fn dispatch<'db>(db: &'db ProjectDatabase, name: &str) -> Option<ResolvedTarget<'db>> {
    if name.is_empty() {
        let user_pkg = baml_compiler2_hir::package::sole_workspace_package(db);
        return Some(ResolvedTarget::Package(user_pkg));
    }

    // Lowercase primitive/keyword aliases resolve to their builtin `baml`
    // companion class — `string` → `baml.String`, `image` → `baml.media.Image`,
    // `json` → `baml.json.json`. Also handles drilling into a member,
    // `string.length` → `baml.String.length`. Checked before the keyword
    // crosswalk so `baml describe string` shows the class (with its methods),
    // not keyword docs.
    if let Some(target) = baml_ide::resolve_builtin_type_target(db, name) {
        return Some(target);
    }

    // Check for keyword (BAML or TS/JS crosswalk) before package routing.
    if baml_builtins2::has_describe_topic(name) {
        return Some(ResolvedTarget::Keyword(name.to_string()));
    }

    // Force user-package resolution with `root.` prefix.
    if let Some(rest) = name.strip_prefix("root.") {
        let user_pkg = baml_compiler2_hir::package::sole_workspace_package(db);
        return baml_ide::resolve_target(db, user_pkg, rest);
    }

    let (first, rest) = name.split_once('.').unwrap_or((name, ""));

    // Builtin package shadows user namespace with same name.
    let builtin_packages = baml_ide::non_workspace_package_names(db);
    if builtin_packages.iter().any(|pkg| pkg.as_str() == first) {
        let pkg = baml_compiler2_hir::package::PackageId::new(db, baml_db::Name::new(first));
        return if rest.is_empty() {
            Some(ResolvedTarget::Package(pkg))
        } else {
            baml_ide::resolve_target(db, pkg, rest)
        };
    }

    // User package.
    let user_pkg = baml_compiler2_hir::package::sole_workspace_package(db);
    if let Some(target) = baml_ide::resolve_target(db, user_pkg, name) {
        return Some(target);
    }

    resolve_unqualified_builtin_member(db, name)
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
    baml_ide::resolve_target(db, baml_pkg, name)
}

impl DescribeArgs {
    /// Run the describe command and return the CLI exit code.
    pub fn run(&self) -> Result<crate::ExitCode> {
        // Introspection never requires a `baml.toml`: with no project, we
        // fall back to a stdlib-only "default state" so `baml describe
        // baml.String` works anywhere. An empty user-file set is therefore
        // expected, not an error — unresolved names still surface through
        // the per-target "No symbol found" + did-you-mean paths below.
        let mut session = crate::project_session::ProjectSession::open_lenient(
            self.from.as_deref(),
            crate::project_session::CacheUse::ReadOnly,
        )?;
        // Warm seeds (no-delta only) + parallel index prime: describe queries
        // the whole-package aggregates, which otherwise derive serially.
        let _ = session.warm_prep_seeds_only();
        session.prime();
        let (db, from) = (session.db, session.resolved.root);

        // ── --symbols deprecation ───────────────────────────────────────────
        if self.symbols {
            eprintln!(
                "warning: `--symbols` is deprecated. Use `baml describe` with no arguments instead."
            );
        }

        let name = self.name.as_deref().unwrap_or("");

        // ── --search: names and docstrings, rather than name resolution ─────
        if self.search {
            let mut packages = vec![baml_compiler2_hir::package::sole_workspace_package(&db)];
            packages.extend(
                baml_ide::non_workspace_package_names(&db)
                    .into_iter()
                    .map(|pkg| baml_compiler2_hir::package::PackageId::new(&db, pkg)),
            );
            let hits = baml_ide::search_ranked(&db, &packages, name, usize::from(self.limit));
            if self.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&hits)
                        .unwrap_or_else(|_| unreachable!("search hits serialize"))
                );
            } else if hits.is_empty() {
                // Not an error: "nothing does this" is a real answer about the
                // standard library, and the caller asked a question rather than
                // named something that should exist. Fall back to the name
                // suggestions, which are fuzzy where this is literal — a query
                // for `iterate` matches no docstring, because they all say
                // "iterator", but it is close enough to a name to be offered.
                println!("no symbol matches: {name}");
                print_did_you_mean(&db, name);
            } else {
                for hit in &hits {
                    let summary = hit
                        .summary
                        .as_deref()
                        .map(|s| format!("  // {s}"))
                        .unwrap_or_default();
                    println!("{:<16} {:<40}{summary}", hit.kind, hit.path);
                }
            }
            return Ok(crate::ExitCode::Success);
        }

        // ── --export: the whole-package surface document ────────────────────
        if self.export {
            let Some(ResolvedTarget::Package(package)) = dispatch(&db, name) else {
                eprintln!(
                    "error: `--export` takes a package name (`baml`, `user`, …), got `{name}`"
                );
                return Ok(crate::ExitCode::Other);
            };
            let export = baml_ide::export_package(&db, package);
            println!(
                "{}",
                serde_json::to_string_pretty(&export)
                    .unwrap_or_else(|_| unreachable!("export IR serializes"))
            );
            return Ok(crate::ExitCode::Success);
        }

        let target = dispatch(&db, name);

        match target {
            Some(ResolvedTarget::Keyword(ref kw)) => {
                if self.json {
                    let json = render_keyword_json(kw);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json)
                            .context("failed to serialize keyword output as JSON")?
                    );
                } else {
                    render_keyword(kw);
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Package(pkg)) => {
                let entries = baml_ide::list_package_items(&db, pkg);
                if entries.is_empty() {
                    eprintln!("no symbols found");
                    return Ok(crate::ExitCode::Other);
                }
                if self.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                            .context("failed to serialize output as JSON")?
                    );
                } else {
                    render_listing(&entries, &from);
                }
                // Check for name collision: does the user also have an item
                // matching the package name? Only hint when the resolved package
                // is a builtin (i.e., the bare name matches a non-user package).
                let pkg_name = name.split('.').next().unwrap_or(name);
                let builtin_names = baml_ide::non_workspace_package_names(&db);
                if builtin_names.iter().any(|pkg| pkg.as_str() == pkg_name) {
                    let user_pkg = baml_compiler2_hir::package::sole_workspace_package(&db);
                    if baml_ide::resolve_target(&db, user_pkg, pkg_name).is_some() {
                        eprintln!();
                        eprintln!(
                            "note: your project also defines `{pkg_name}`. \
                             Use `baml describe root.{pkg_name}` to see your definition."
                        );
                    }
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Namespace { package, ns_path }) => {
                let entries =
                    baml_ide::list_namespace_items(&db, package, &ns_path).unwrap_or_default();
                if entries.is_empty() {
                    eprintln!("no symbols found in namespace");
                    return Ok(crate::ExitCode::Other);
                }
                if self.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&listing_to_json(&db, &entries, &from))
                            .context("failed to serialize output as JSON")?
                    );
                } else {
                    render_listing(&entries, &from);
                }
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Item(def)) => {
                let files = baml_compiler2_hir::compiler2_all_files(&db);
                let Some(desc) = describe::describe_by_definition(&db, &files, def) else {
                    eprintln!("no symbol found: {name}");
                    print_did_you_mean(&db, name);
                    return Ok(crate::ExitCode::Other);
                };
                self.emit_description(&db, &desc, &from)?;
                Ok(crate::ExitCode::Success)
            }
            Some(ResolvedTarget::Member {
                parent,
                member_name,
            }) => {
                let files = baml_compiler2_hir::compiler2_all_files(&db);
                let Some(desc) =
                    describe::describe_item_member(&db, &files, parent, member_name.as_str())
                else {
                    eprintln!("no symbol found: {name}");
                    print_did_you_mean(&db, name);
                    return Ok(crate::ExitCode::Other);
                };
                self.emit_description(&db, &desc, &from)?;
                Ok(crate::ExitCode::Success)
            }
            None => {
                // Exact-name fallback: an unqualified name may live in any
                // namespace of any package (`Point` declared under
                // `shapes/`), or be a local (parameter, let binding). Scan
                // the compiler-visible files and show every match.
                let files = baml_compiler2_hir::compiler2_all_files(&db);
                let matches = describe::describe(&db, &files, name);

                if matches.is_empty() {
                    eprintln!("no symbol found: {name}");
                    print_did_you_mean(&db, name);
                    return Ok(crate::ExitCode::Other);
                }

                if self.json {
                    let documents: Vec<serde_json::Value> = matches
                        .iter()
                        .map(|desc| description_to_json(&db, desc, self.budget, &from))
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&documents)
                            .context("failed to serialize output as JSON")?
                    );
                    return Ok(crate::ExitCode::Success);
                }

                for (i, desc) in matches.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    render_description(&db, desc, self.budget, &from);
                }

                Ok(crate::ExitCode::Success)
            }
        }
    }

    /// Print one description in the selected output mode.
    fn emit_description(
        &self,
        db: &ProjectDatabase,
        desc: &SymbolDescription,
        project_root: &std::path::Path,
    ) -> Result<()> {
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&description_to_json(
                    db,
                    desc,
                    self.budget,
                    project_root
                ))
                .context("failed to serialize output as JSON")?
            );
        } else {
            render_description(db, desc, self.budget, project_root);
        }
        Ok(())
    }
}

/// Render keyword documentation to a writer.
pub fn write_keyword(w: &mut impl std::io::Write, name: &str) -> std::io::Result<()> {
    let painter = crate::paint::Painter::stdout();
    if let Some(doc) = baml_builtins2::language_topic(name) {
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
    } else if let Some(doc) = baml_builtins2::typescript_crosswalk_topic(name) {
        writeln!(w, "{} — {}", painter.keyword(name), doc.message)?;
        if let Some(ref see) = doc.see {
            writeln!(w)?;
            writeln!(w, "see: `baml describe {}`", painter.fragment(see))?;
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
    if let Some(doc) = baml_builtins2::language_topic(name) {
        serde_json::json!({
            "type": "keyword",
            "name": name,
            "summary": doc.summary,
            "syntax": doc.syntax,
            "details": doc.details,
        })
    } else if let Some(doc) = baml_builtins2::typescript_crosswalk_topic(name) {
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

// The full-output hint is computed before sections are rendered, so fixed
// layout groups have explicit costs shared by the planner and renderer.
const ITEM_HEADER_COST: usize = 1;
const SECTION_HEADER_COST: usize = 2;
const CONTAINER_SECTION_COST: usize = 3;
const LIST_ENTRY_COST: usize = 1;

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
    let definition_kind = desc.kind.definition_kind();
    let kind_str = definition_kind.as_str();
    let rel_path = relative_path(&file_path, project_root);
    let painter = crate::paint::Painter::stdout();
    let colors = painter.enabled();
    let hl = crate::paint::Highlighter::new(db);
    let fqn_part = desc
        .kind
        .canonical_fqn()
        .map(|f| format!("  ({})", painter.fqn(f, definition_kind)))
        .unwrap_or_default();
    let name_display = painter.fqn(&desc.name, definition_kind);
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

    let mut lines_used = ITEM_HEADER_COST;

    // ── Body ─────────────────────────────────────────────────────────────────
    // The body slice already includes any leading `///` doc-comments
    // (the CST parser attaches them as the first children of the item
    // node, so they fall inside `node.text_range()` and round-trip
    // through `slice_text`). Rendering `desc.docstring` separately
    // would just duplicate the same lines, so leave the docstring
    // surfacing to JSON consumers and let the body show it once.
    let is_local = matches!(desc.kind, baml_ide::SymbolKind::Local { .. });
    // Interfaces render from the structured member surface (facet-layered),
    // never from a raw source slice, so the highlighted-slice machinery is
    // skipped for them.
    let interface = match &desc.kind {
        baml_ide::SymbolKind::Interface {
            members,
            implementations,
            ..
        } => Some((members.as_slice(), implementations.as_slice())),
        _ => None,
    };
    let body_lines: Vec<&str> = desc.full_body.lines().collect();
    let highlighted_body =
        (colors && interface.is_none()).then(|| hl.range(desc.file, desc.item_range));
    let highlighted_body_lines = highlighted_body.as_deref().map(trim_blank_edge_lines);
    let body_line_count = if let Some((members, _)) = interface {
        interface_full_line_count(desc, members)
    } else {
        highlighted_body_lines
            .as_ref()
            .map_or(body_lines.len(), Vec::len)
    };
    let full_output_budget = minimum_full_output_budget(desc, body_line_count, is_local);

    if let Some((members, implementations)) = interface {
        writeln!(w)?;
        let available_for_body = budget.saturating_sub(lines_used);
        lines_used += write_interface_body(
            w,
            &painter,
            desc,
            members,
            implementations,
            available_for_body,
            full_output_budget,
        )?;
    } else if !is_local {
        // The separator blank line is deliberately not counted against the
        // budget: the body's available-line computation predates the section
        // budgeting below and is a documented guarantee (a fields-only class
        // body must fit at budget 5).
        writeln!(w)?;
        let available_for_body = budget.saturating_sub(lines_used);

        // Colored TTY output renders the verbatim definition slice through the
        // compiler's semantic tokens. Plain output (pipes, JSON, tests) uses
        // the cleaned body representation.
        if let Some(lines) = highlighted_body_lines.as_deref() {
            lines_used += write_highlighted_body(w, lines, available_for_body, full_output_budget)?;
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
                    "[INFO] showing {shown} of {total} lines; use `--budget {needed}` for full output",
                    shown = shown_body_lines,
                    total = body_lines.len(),
                    needed = full_output_budget,
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
    let mut render_budget =
        RenderBudget::new(budget.saturating_sub(lines_used), full_output_budget);

    // ── Methods (classes) ────────────────────────────────────────────────────
    if let baml_ide::SymbolKind::Class {
        instance_methods,
        static_methods,
        ..
    } = &desc.kind
    {
        write_method_section(
            w,
            db,
            &painter,
            project_root,
            "methods",
            instance_methods,
            &mut render_budget,
        )?;
        write_method_section(
            w,
            db,
            &painter,
            project_root,
            "static_methods",
            static_methods,
            &mut render_budget,
        )?;
    }

    // ── Container ────────────────────────────────────────────────────────────
    // Always shown in full: it's a single entry and part of the symbol's
    // identity, like the header.
    if let Some(c) = desc.kind.container() {
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
        render_budget.consume(CONTAINER_SECTION_COST);
    }

    // ── Dependencies ─────────────────────────────────────────────────────────
    if !desc.dependencies.is_empty() {
        writeln!(w)?;
        writeln!(w, "dependencies:")?;
        render_budget.consume(SECTION_HEADER_COST);
        let mut elided = 0usize;
        for dep in &desc.dependencies {
            if !render_budget.can_start_atomic() {
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
            render_budget.consume(LIST_ENTRY_COST);
        }
        write_elision_marker(w, elided, render_budget.full_output)?;
    }

    // ── Implementations ──────────────────────────────────────────────────────
    // Interfaces: the impl blocks whose head names this interface — the "who
    // implements it" list, separate from references (which no longer repeat
    // the head mentions these rows own).
    if let Some((_, implementations)) = interface.filter(|(_, imps)| !imps.is_empty()) {
        writeln!(w)?;
        writeln!(w, "implementations ({}):", implementations.len())?;
        render_budget.consume(SECTION_HEADER_COST);
        let mut elided = 0usize;
        for imp in implementations {
            if !render_budget.can_start_atomic() {
                elided += 1;
                continue;
            }
            let imp_abs = imp.file.path(db);
            let imp_path = relative_path(&imp_abs, project_root);
            let imp_line = line_number_at_offset(imp.file.text(db), imp.span.start().into());
            let loc = painter.location(
                &imp_abs,
                &imp_path.display().to_string(),
                &imp_line.to_string(),
            );
            writeln!(w, "  {}  {loc}", styled_declaration(&painter, &imp.display))?;
            render_budget.consume(LIST_ENTRY_COST);
        }
        write_elision_marker(w, elided, render_budget.full_output)?;
    }

    // ── References ───────────────────────────────────────────────────────────
    // Lowest priority: references are the first thing to give way under a
    // tight budget. The header always shows the total count.
    writeln!(w)?;
    writeln!(w, "references ({}):", desc.references.len())?;
    render_budget.consume(SECTION_HEADER_COST);
    let mut elided = 0usize;
    for r in &desc.references {
        if !render_budget.can_start_atomic() {
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
        render_budget.consume(LIST_ENTRY_COST);
    }
    write_elision_marker(w, elided, render_budget.full_output)?;

    Ok(())
}

/// Every line an interface body renders at full disclosure: the item
/// docstring, the block skeleton (header + one declaration per member +
/// closing brace), each member docstring, and each defaulted body's extra
/// lines beyond the declaration it replaces. Shared by the renderer's
/// truncation report and the full-output budget planner.
fn interface_full_line_count(
    desc: &SymbolDescription,
    members: &[baml_ide::InterfaceMember],
) -> usize {
    let doc_lines = doc_line_count(desc.docstring.as_deref());
    let facets: usize = members
        .iter()
        .map(|member| doc_line_count(member.docstring.as_deref()) + member_body_extra_lines(member))
        .sum();
    doc_lines + interface_skeleton_lines(desc, members) + facets
}

/// The always-rendered part of the interface body: header + one declaration
/// line per member + closing brace. An interface with no structured members
/// (empty, or a resolver-less fallback) falls back to its shape's line count.
fn interface_skeleton_lines(
    desc: &SymbolDescription,
    members: &[baml_ide::InterfaceMember],
) -> usize {
    if members.is_empty() {
        desc.shape.lines().count()
    } else {
        2 + members.len()
    }
}

fn doc_line_count(docstring: Option<&str>) -> usize {
    docstring.map_or(0, |doc| doc.lines().count())
}

/// A defaulted body's cost beyond the declaration line it replaces.
fn member_body_extra_lines(member: &baml_ide::InterfaceMember) -> usize {
    member
        .body
        .as_deref()
        .map_or(0, |body| body.lines().count().saturating_sub(1))
}

/// Style a one-line declaration that is not parseable on its own — a bare
/// method signature, an `implement … for …` head — by classifying it with an
/// empty `{}` body appended and stripping the body back off the styled text.
/// The fragment highlighter refuses text that does not parse, and these rows
/// are body-less forms only interface bodies (or nothing) admit; `{}` is
/// punctuation the classifier never styles, so it survives the round trip
/// verbatim — including through the plain fallback for a declaration that
/// still does not parse.
fn styled_declaration(painter: &crate::paint::Painter, declaration: &str) -> String {
    let styled = painter.fragment(&format!("{declaration} {{}}"));
    match styled.strip_suffix(" {}") {
        Some(bare) => bare.to_string(),
        None => styled,
    }
}

/// The full cost of the sections that OUTRANK a defaulted body's disclosure —
/// dependencies and implementations; not references, which stay the lowest
/// priority overall (an interface is a top-level item, so it structurally has
/// no container section). A body is an interface's least important facet
/// (the ruling: signature, then docstring, then body), so the body layer
/// admits only from budget these sections will not consume.
fn interface_higher_section_cost(
    desc: &SymbolDescription,
    implementations: &[baml_ide::ImplRow],
) -> usize {
    let mut cost = 0;
    if !desc.dependencies.is_empty() {
        cost += SECTION_HEADER_COST + desc.dependencies.len() * LIST_ENTRY_COST;
    }
    if !implementations.is_empty() {
        cost += SECTION_HEADER_COST + implementations.len() * LIST_ENTRY_COST;
    }
    cost
}

/// Render an interface body with facet-layered budgeting.
///
/// The member ENUMERATION — header, one declaration per member, closing
/// brace — always renders in full: it is the interface's identity, and a
/// complete listing is what lets the reader drill into a specific member
/// instead of paging the whole thing. Docstrings are then admitted while
/// budget remains — the item's own first, then the members' in declaration
/// priority order (associated types, fields, required methods, defaulted
/// methods) — and defaulted bodies last, replacing their `{ ... }`-marked
/// declaration line. A facet is atomic: it renders whole or not at all (it
/// may start on the last remaining line and run over, the same rule the
/// list sections use), so a docstring is never cut mid-sentence.
///
/// Colored output classifies the assembled block as ONE fragment — the
/// reconstruction is a valid interface declaration, where any single line
/// in isolation is not, and the fragment highlighter refuses text that
/// does not parse.
///
/// Returns the number of lines written (excluding the caller's separator
/// blank line).
fn write_interface_body(
    w: &mut impl std::io::Write,
    painter: &crate::paint::Painter,
    desc: &SymbolDescription,
    members: &[baml_ide::InterfaceMember],
    implementations: &[baml_ide::ImplRow],
    available: usize,
    full_output_budget: usize,
) -> std::io::Result<usize> {
    // Facet admission. `remaining` covers only the disclosure layers; the
    // skeleton is unconditional.
    let mut remaining = available.saturating_sub(interface_skeleton_lines(desc, members));
    let admit = |cost: usize, remaining: &mut usize| -> bool {
        // A zero-cost facet (an absent docstring; a one-line body that costs
        // nothing over its declaration) is always disclosed.
        if cost == 0 {
            return true;
        }
        if *remaining == 0 {
            return false;
        }
        *remaining = remaining.saturating_sub(cost);
        true
    };
    let item_doc_admitted = admit(doc_line_count(desc.docstring.as_deref()), &mut remaining);
    let docs_admitted: Vec<bool> = members
        .iter()
        .map(|member| admit(doc_line_count(member.docstring.as_deref()), &mut remaining))
        .collect();
    // Bodies are the lowest disclosure layer: they admit only from budget the
    // higher-ranked sections below the block will not consume, and only when
    // they fit WHOLE — the `{ ... }`-marked declaration already shows, so an
    // atomic-start overshoot would buy nothing a drill-in doesn't.
    let mut body_budget =
        remaining.saturating_sub(interface_higher_section_cost(desc, implementations));
    let bodies_admitted: Vec<bool> = members
        .iter()
        .map(|member| {
            let cost = member_body_extra_lines(member);
            if cost <= body_budget {
                body_budget -= cost;
                true
            } else {
                false
            }
        })
        .collect();

    // The fragment highlighter refuses text that does not parse, and every
    // line here is un-parseable in isolation — so the block is assembled
    // FIRST and classified ONCE: the reconstruction (docstring, header,
    // member declarations, admitted bodies) is itself a valid interface
    // declaration. The one non-syntax piece — a defaulted method's `{ ... }`
    // marker — is held out of the classified text (its bodyless signature is
    // valid interface syntax) and re-appended per line after styling, which
    // is safe because no styling run ever crosses a newline.
    let mut lines: Vec<(String, bool)> = Vec::new();
    fn push_doc(lines: &mut Vec<(String, bool)>, doc: &str, indent: &str) {
        for line in doc.lines() {
            let text = if line.is_empty() {
                format!("{indent}///")
            } else {
                format!("{indent}/// {line}")
            };
            lines.push((text, false));
        }
    }

    if item_doc_admitted && let Some(doc) = desc.docstring.as_deref() {
        push_doc(&mut lines, doc, "");
    }

    if members.is_empty() {
        // No structured members: the shape (bare header, or a resolver-less
        // fallback) is the whole block.
        lines.extend(desc.shape.lines().map(|line| (line.to_string(), false)));
    } else {
        // The header line (`interface Foo<T> requires Bar {`) is the shape's
        // first line; declarations and the close come from the structured
        // surface, so the two renderings cannot disagree about a member.
        let header = desc.shape.lines().next().unwrap_or_default();
        lines.push((header.to_string(), false));
        for (i, member) in members.iter().enumerate() {
            if docs_admitted[i]
                && let Some(doc) = member.docstring.as_deref()
            {
                push_doc(&mut lines, doc, "    ");
            }
            if bodies_admitted[i]
                && let Some(body) = member.body.as_deref()
            {
                lines.extend(body.lines().map(|line| (line.to_string(), false)));
            } else if let Some(bare) = member.declaration.strip_suffix(" { ... }") {
                // Only a defaulted method's declaration carries the marker;
                // stripping it here is what re-appends it below.
                lines.push((format!("    {bare}"), true));
            } else {
                lines.push((format!("    {}", member.declaration), false));
            }
        }
        lines.push(("}".to_string(), false));
    }

    let classified = painter.fragment(
        &lines
            .iter()
            .map(|(text, _)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    debug_assert_eq!(
        classified.split('\n').count(),
        lines.len().max(1),
        "fragment styling preserves line structure"
    );
    for (styled, (_, marker)) in classified.split('\n').zip(&lines) {
        if *marker {
            writeln!(w, "{styled} {{ ... }}")?;
        } else {
            writeln!(w, "{styled}")?;
        }
    }
    let mut printed = lines.len();

    let total = interface_full_line_count(desc, members);
    if printed < total {
        writeln!(w)?;
        writeln!(
            w,
            "[INFO] showing {printed} of {total} lines; use `--budget {full_output_budget}` for full output",
        )?;
        printed += 2;
    }
    Ok(printed)
}

/// Render the definition body with ANSI highlighting (colored-output path).
///
/// Highlights the verbatim source slice of the item via the compiler's semantic
/// tokens, then applies the soft line budget by line count (so an ANSI escape
/// run is never split). Returns the number of output lines consumed.
fn write_highlighted_body(
    w: &mut impl std::io::Write,
    lines: &[&str],
    available_for_body: usize,
    full_output_budget: usize,
) -> std::io::Result<usize> {
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
        "[INFO] showing {} of {} lines; use `--budget {}` for full output",
        head + tail + 1,
        lines.len(),
        full_output_budget,
    )?;
    Ok(head + tail + 3)
}

fn trim_blank_edge_lines(text: &str) -> Vec<&str> {
    let all: Vec<&str> = text.lines().collect();
    let first = all
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(0);
    let last = all
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(first, |end| end + 1);
    all[first..last].to_vec()
}

/// Tracks the minimum starting budget needed to render every guarded unit.
///
/// Soft overhead is always printed, even after the budget is exhausted, so it
/// only advances `consumed`. A required block must fit in full. An atomic unit
/// may start whenever one line remains, even when its full cost is larger.
#[derive(Default)]
struct BudgetRequirement {
    consumed: usize,
    minimum: usize,
}

struct RenderBudget {
    remaining: usize,
    full_output: usize,
}

impl RenderBudget {
    fn new(remaining: usize, full_output: usize) -> Self {
        Self {
            remaining,
            full_output,
        }
    }

    fn consume(&mut self, cost: usize) {
        self.remaining = self.remaining.saturating_sub(cost);
    }

    fn can_start_atomic(&self) -> bool {
        self.remaining > 0
    }
}

impl BudgetRequirement {
    fn add_soft_overhead(&mut self, cost: usize) {
        self.consumed = self.consumed.saturating_add(cost);
    }

    fn add_required(&mut self, cost: usize) {
        self.add_soft_overhead(cost);
        self.minimum = self.minimum.max(self.consumed);
    }

    fn add_atomic(&mut self, cost: usize) {
        debug_assert!(cost > 0);
        self.minimum = self.minimum.max(self.consumed.saturating_add(1));
        self.add_soft_overhead(cost);
    }
}

fn minimum_full_output_budget(
    desc: &SymbolDescription,
    body_line_count: usize,
    is_local: bool,
) -> usize {
    let mut required = BudgetRequirement::default();

    if is_local {
        required.add_soft_overhead(ITEM_HEADER_COST);
    } else {
        required.add_required(ITEM_HEADER_COST.saturating_add(body_line_count));
    }

    if let baml_ide::SymbolKind::Class {
        instance_methods,
        static_methods,
        ..
    } = &desc.kind
    {
        add_method_budget(&mut required, instance_methods);
        add_method_budget(&mut required, static_methods);
    }

    if desc.kind.container().is_some() {
        required.add_soft_overhead(CONTAINER_SECTION_COST);
    }

    if !desc.dependencies.is_empty() {
        required.add_soft_overhead(SECTION_HEADER_COST);
        for _ in &desc.dependencies {
            required.add_atomic(LIST_ENTRY_COST);
        }
    }

    if let baml_ide::SymbolKind::Interface {
        implementations, ..
    } = &desc.kind
        && !implementations.is_empty()
    {
        required.add_soft_overhead(SECTION_HEADER_COST);
        for _ in implementations {
            required.add_atomic(LIST_ENTRY_COST);
        }
    }

    required.add_soft_overhead(SECTION_HEADER_COST);
    for _ in &desc.references {
        required.add_atomic(LIST_ENTRY_COST);
    }

    required.minimum
}

fn add_method_budget(required: &mut BudgetRequirement, methods: &[describe::MethodRef]) {
    if !methods.is_empty() {
        required.add_soft_overhead(SECTION_HEADER_COST);
        for method in methods {
            required.add_atomic(method_line_cost(method));
        }
    }
}

fn method_line_cost(method: &describe::MethodRef) -> usize {
    1 + usize::from(method.docstring.is_some())
}

/// Write the soft-budget elision marker for `elided` hidden lines (no-op when
/// nothing was elided).
fn write_elision_marker(
    w: &mut impl std::io::Write,
    elided: usize,
    full_output_budget: usize,
) -> std::io::Result<()> {
    if elided > 0 {
        writeln!(
            w,
            "  \u{2026} {elided} more lines (re-run with a higher `--budget` to see more; use `--budget {full_output_budget}` for full output)"
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
/// from the shared soft line `budget`: the header is always emitted, each
/// method is an atomic unit (docstring + signature are never split, even if
/// the last one runs slightly over), and methods that don't fit are summarized
/// by an elision marker.
fn write_method_section(
    w: &mut impl std::io::Write,
    db: &ProjectDatabase,
    painter: &crate::paint::Painter,
    project_root: &std::path::Path,
    label: &str,
    methods: &[describe::MethodRef],
    budget: &mut RenderBudget,
) -> std::io::Result<()> {
    if methods.is_empty() {
        return Ok(());
    }
    writeln!(w)?;
    writeln!(w, "{label}:")?;
    budget.consume(SECTION_HEADER_COST);
    let mut elided_lines = 0usize;
    for m in methods {
        let unit_cost = method_line_cost(m);
        if !budget.can_start_atomic() {
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
        let sig = styled_declaration(painter, &m.signature);
        writeln!(w, "  {sig}  {loc}")?;
        budget.consume(unit_cost);
    }
    write_elision_marker(w, elided_lines, budget.full_output)?;
    Ok(())
}

/// Render a flat listing of entries to stdout.
fn render_listing(entries: &[baml_ide::ListingEntry], project_root: &std::path::Path) {
    let _ = write_listing(&mut std::io::stdout(), entries, project_root);
}

/// Render a flat listing of entries to a writer.
pub fn write_listing(
    w: &mut impl std::io::Write,
    entries: &[baml_ide::ListingEntry],
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
    kind: baml_ide::DefinitionKind,
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
    kind: baml_ide::DefinitionKind,
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
    entries: &[baml_ide::ListingEntry],
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

fn budget_body(desc: &SymbolDescription, budget: usize) -> String {
    let body_lines: Vec<&str> = desc.full_body.lines().collect();

    if body_lines.len() <= budget {
        desc.full_body.clone()
    } else if budget >= 5 {
        truncate_body(&body_lines, budget).join("\n")
    } else {
        shape_with_elision(&desc.shape, &desc.full_body)
    }
}

fn method_json(
    db: &ProjectDatabase,
    project_root: &std::path::Path,
    methods: &[baml_ide::MethodRef],
) -> Vec<serde_json::Value> {
    methods
        .iter()
        .map(|method| {
            let path = relative_path(&method.file.path(db), project_root);
            let text = method.file.text(db);
            serde_json::json!({
                "name": method.name,
                "signature": method.signature,
                "docstring": method.docstring,
                "file": path.to_string_lossy(),
                "line_start": line_number_at_offset(text, method.item_range.start().into()),
                "line_end": line_number_at_offset(text, method.item_range.end().into()),
            })
        })
        .collect()
}

fn description_to_json(
    db: &ProjectDatabase,
    desc: &SymbolDescription,
    budget: usize,
    project_root: &std::path::Path,
) -> serde_json::Value {
    let file_path = relative_path(&desc.file.path(db), project_root);
    let body = budget_body(desc, budget);
    // The wire shape keeps every section field present (empty for kinds that
    // don't carry it), so consumers never branch on absence.
    let (instance_methods, static_methods): (&[baml_ide::MethodRef], &[baml_ide::MethodRef]) =
        match &desc.kind {
            baml_ide::SymbolKind::Class {
                instance_methods,
                static_methods,
                ..
            } => (instance_methods, static_methods),
            _ => (&[], &[]),
        };
    let (interface_members, implementations): (&[baml_ide::InterfaceMember], &[baml_ide::ImplRow]) =
        match &desc.kind {
            baml_ide::SymbolKind::Interface {
                members,
                implementations,
                ..
            } => (members, implementations),
            _ => (&[], &[]),
        };
    serde_json::json!({
        "name": desc.name,
        "kind": desc.kind.definition_kind().as_str(),
        "file": file_path.to_string_lossy(),
        "line": line_number_at_offset(desc.file.text(db), desc.name_span.start().into()),
        "shape": desc.shape,
        "body": body,
        "docstring": desc.docstring,
        "resolved_type": desc.resolved_type,
        "dependencies": desc.dependencies.iter().map(|dep| {
            let dep_path = relative_path(&dep.file.path(db), project_root);
            serde_json::json!({
                "name": dep.name,
                "kind": dep.kind.as_str(),
                "file": dep_path.to_string_lossy(),
                "line": line_number_at_offset(dep.file.text(db), dep.name_span.start().into()),
            })
        }).collect::<Vec<_>>(),
        "references": desc.references.iter().map(|reference| {
            let ref_path = relative_path(&reference.file.path(db), project_root);
            serde_json::json!({
                "file": ref_path.to_string_lossy(),
                "line": reference.line_number,
                "text": reference.line_text.trim(),
            })
        }).collect::<Vec<_>>(),
        "instance_methods": method_json(db, project_root, instance_methods),
        "static_methods": method_json(db, project_root, static_methods),
        "interface_members": interface_members.iter().map(|member| {
            let path = relative_path(&member.file.path(db), project_root);
            let text = member.file.text(db);
            serde_json::json!({
                "category": member.category,
                "name": member.name,
                "declaration": member.declaration,
                "docstring": member.docstring,
                "body": member.body,
                "file": path.to_string_lossy(),
                "line_start": line_number_at_offset(text, member.item_range.start().into()),
                "line_end": line_number_at_offset(text, member.item_range.end().into()),
            })
        }).collect::<Vec<_>>(),
        "implementations": implementations.iter().map(|imp| {
            let path = relative_path(&imp.file.path(db), project_root);
            serde_json::json!({
                "display": imp.display,
                "file": path.to_string_lossy(),
                "line": line_number_at_offset(imp.file.text(db), imp.span.start().into()),
            })
        }).collect::<Vec<_>>(),
        "container": desc.kind.container().map(|container| {
            let path = relative_path(&container.file.path(db), project_root);
            serde_json::json!({
                "name": container.name,
                "kind": container.kind.as_str(),
                "file": path.to_string_lossy(),
                "line": line_number_at_offset(container.file.text(db), container.name_span.start().into()),
            })
        }),
    })
}
