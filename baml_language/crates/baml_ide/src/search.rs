//! One symbol-search engine, two modes.
//!
//! - [`search_symbols`] — case-insensitive *substring* match over
//!   [`crate::outline::file_outline`], for `workspace/symbol` (editors filter
//!   and rank client-side, so the server's job is candidate supply).
//! - [`search_ranked`] — whole-*word* ranked search over names AND
//!   docstrings, for `baml describe --search`, did-you-mean, and topic
//!   queries. Substrings are how a search becomes noise (`run` is inside
//!   `trunc`); prose is where do-what-I-mean knowledge lives (`baml.fs.read`
//!   is findable by "read a file" because its documentation says so).
//!
//! The ranked mode is lexical, not semantic, and the limit is worth stating
//! plainly: a query only finds a symbol that shares a word with its name or
//! its prose. Reaching past that needs a semantic index, which this is not.

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::{Definition, DefinitionKind},
    package::{PackageId, package_files, package_items},
};
use baml_compiler2_ppir::item_data;
use text_size::TextRange;

use crate::outline::{OutlineItem, file_outline};
use crate::symbols::is_synthesized;

// ── Substring mode (workspace/symbol) ────────────────────────────────────────

/// A symbol result from workspace-wide substring search: everything the LSP
/// layer needs to build a `WorkspaceSymbol` response.
#[derive(Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: DefinitionKind,
    pub file: SourceFile,
    /// Byte range of the name token.
    pub name_span: TextRange,
    /// The enclosing symbol's name, for members (`containerName`).
    pub container_name: Option<String>,
}

/// Search `files` for symbols whose name contains `query`
/// (ASCII-case-insensitively). An empty query matches every symbol
/// (workspace symbol browsing).
///
/// Callers choose the visible file set for their feature — typically every
/// `Workspace` root's files, or `compiler2_all_files` to include the stdlib.
pub fn search_symbols(
    db: &dyn baml_compiler2_ppir::Db,
    files: &[SourceFile],
    query: &str,
) -> Vec<SymbolInfo> {
    let mut results: Vec<SymbolInfo> = Vec::new();
    for &file in files {
        db.unwind_if_revision_cancelled();
        for item in file_outline(db, file) {
            collect_symbol_matches(file, item, None, query, &mut results);
        }
    }
    results
}

fn collect_symbol_matches(
    file: SourceFile,
    item: &OutlineItem,
    container: Option<&str>,
    query: &str,
    results: &mut Vec<SymbolInfo>,
) {
    if contains_ignore_ascii_case(&item.name, query) {
        results.push(SymbolInfo {
            name: item.name.clone(),
            kind: item.kind,
            file,
            name_span: item.name_span,
            container_name: container.map(str::to_owned),
        });
    }
    for child in &item.children {
        collect_symbol_matches(file, child, Some(&item.name), query, results);
    }
}

/// Substring containment ignoring ASCII case, without allocating a lowercase
/// copy of every candidate name per query (BAML identifiers are ASCII; a
/// non-ASCII needle simply matches exactly).
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

// ── Ranked word-search mode ──────────────────────────────────────────────────

/// One ranked result: what it is, how to name it, and enough of what it does
/// to decide whether to look closer. `path` is addressable — pasting it back
/// into `baml describe` resolves it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SearchHit {
    pub kind: &'static str,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub score: u32,
}

/// Whole-word ranked search over the items, members, and impl methods of
/// `packages`. Results are sorted best-first (ties by path) and truncated to
/// `limit`.
pub fn search_ranked(
    db: &dyn baml_compiler2_ppir::Db,
    packages: &[PackageId<'_>],
    query: &str,
    limit: usize,
) -> Vec<SearchHit> {
    let query = query_tokens(query);
    if query.is_empty() {
        return Vec::new();
    }

    let mut hits: Vec<SearchHit> = Vec::new();
    for candidate in ranked_candidates(db, packages) {
        let leaf = haystack(&candidate.leaf);
        let owner = haystack(&candidate.path);
        let docs = candidate
            .docstring
            .as_deref()
            .map(haystack)
            .unwrap_or_default();
        let score = score(&leaf, &owner, &docs, &query);
        if score > 0 {
            hits.push(SearchHit {
                kind: candidate.kind,
                path: candidate.path,
                summary: summarize(candidate.docstring.as_ref()),
                score,
            });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(limit);
    hits
}

/// Words so common that scoring them is noise rather than signal.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "from", "into", "with", "that", "this", "there", "then", "when", "how",
    "does", "using", "use", "get", "set", "all", "any", "not",
];

/// A tokenized query, split by what each half may be matched against.
///
/// Two lists rather than one because filtering serves ranking and defeats
/// lookup. `get` is too common to rank a docstring by and is exactly the
/// right thing to match a name against, and a single list has to choose.
#[derive(Debug)]
struct Query {
    /// Tokens that discriminate: prose and owner are ranked on these.
    relevance: Vec<String>,
    /// Tokens that might be a symbol's name, whole-word.
    names: Vec<String>,
}

impl Query {
    fn is_empty(&self) -> bool {
        self.relevance.is_empty() && self.names.is_empty()
    }
}

/// The words a query is looking for, tokenized through [`haystack`] the same
/// way the searched text is — so `ZonedDateTime` typed as a query matches
/// the camelCase-split ` zoned date time ` of the name.
///
/// One- and two-character tokens and stop words are dropped from
/// `relevance` — but not from `names`, because a symbol may simply *be*
/// called `get`. Single characters stay out of both: camelCase splitting
/// turns `toA` into ` to a `, so a stray `a` would match nothing but a
/// tokenizer artefact.
fn query_tokens(query: &str) -> Query {
    let words: Vec<String> = haystack(query)
        .split_whitespace()
        .map(str::to_string)
        .take(8)
        .collect();
    Query {
        relevance: words
            .iter()
            .filter(|token| token.chars().count() > 2 && !STOP_WORDS.contains(&token.as_str()))
            .cloned()
            .collect(),
        names: words
            .into_iter()
            .filter(|token| token.chars().count() > 1)
            .collect(),
    }
}

/// A name or docstring reduced to whole words, space-delimited at both ends
/// so `" token "` asks for an exact word and `" token"` for a word starting
/// with one. Splits on camelCase as well as punctuation, so a query written
/// in one language's casing finds a name written in the other's.
fn haystack(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    let mut previous_upper = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if ch.is_uppercase() && !previous_upper && !out.ends_with(' ') {
                out.push(' ');
            }
            out.extend(ch.to_lowercase());
            previous_upper = ch.is_uppercase();
        } else {
            if !out.ends_with(' ') {
                out.push(' ');
            }
            previous_upper = false;
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

/// How well one candidate answers the query.
///
/// The symbol's own name counts for most, its docstring next, and the path
/// it hangs under least — weighting the whole path equally made every member
/// of a matching type rank with it. A prefix still scores, so `run` finds
/// "Runs" — English inflection is not worth a stemmer, but ignoring it would
/// lose the most natural way to ask.
fn score(leaf: &str, owner: &str, docs: &str, query: &Query) -> u32 {
    let mut total = 0;
    // A name is matched on every token, including the ones too common to
    // rank prose by: symbols really are called `get`, `set`, and `id`.
    for token in &query.names {
        if leaf.contains(&format!(" {token} ")) {
            total += 20;
        }
    }
    for token in &query.relevance {
        let exact = format!(" {token} ");
        let prefix = format!(" {token}");
        if !query.names.contains(token) && leaf.contains(&exact) {
            total += 20;
        } else if leaf.contains(&prefix) && !leaf.contains(&exact) {
            total += 10;
        }
        if docs.contains(&exact) {
            total += 6;
        } else if docs.contains(&prefix) {
            total += 2;
        }
        if owner.contains(&exact) {
            total += 3;
        }
    }
    total
}

/// The first line of a docstring, which is where the summary is by
/// convention.
fn summarize(docstring: Option<&String>) -> Option<String> {
    let text = docstring?.lines().next()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

// ── Candidates ───────────────────────────────────────────────────────────────

/// One thing the ranked search can land on: an item, a member of one, or an
/// impl-block method.
struct Candidate {
    kind: &'static str,
    /// The symbol's own name, which is what a query is usually reaching for.
    leaf: String,
    /// The full dotted path, shown to the reader and scored weakly.
    path: String,
    docstring: Option<String>,
}

/// Everything in `packages` the ranked search can land on.
fn ranked_candidates(
    db: &dyn baml_compiler2_ppir::Db,
    packages: &[PackageId<'_>],
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for &package in packages {
        db.unwind_if_revision_cancelled();
        let prefix = baml_type::addressable_package(&package.name(db)).to_string();
        let items = package_items(db, package);
        for (ns_path, ns_items) in &items.namespaces {
            for (name, def) in ns_items.types.iter().chain(ns_items.values.iter()) {
                if is_synthesized(db, name, *def) {
                    continue;
                }
                let path = dotted(&prefix, ns_path, name.as_str());
                collect_definition_candidates(db, *def, name, &path, &mut out);
            }
        }

        // Methods reached through a free `implements` block hang off the
        // block, not the item: sorting, comparison, iteration, and every
        // operator arrive this way. The path is spelled as the reader would
        // write the call (`T[].sort`), not as the impl is declared.
        for &file in package_files(db, package) {
            db.unwind_if_revision_cancelled();
            for &block in item_data::file_impls(db, file) {
                let data = item_data::impl_block_data(db, block);
                let for_display = match &data.subject {
                    item_data::ImplSubjectData::InClass { class, .. } => {
                        item_data::class_data(db, *class).name.as_str().to_string()
                    }
                    item_data::ImplSubjectData::Free { for_target, .. } => {
                        data.type_refs.display(*for_target).to_string()
                    }
                };
                for &method in &data.methods {
                    let func = item_data::function_data(db, method);
                    out.push(Candidate {
                        kind: "method",
                        leaf: func.name.as_str().to_string(),
                        path: format!("{for_display}.{}", func.name.as_str()),
                        docstring: func.docstring.clone(),
                    });
                }
            }
        }
    }
    out
}

fn dotted(prefix: &str, ns_path: &[Name], name: &str) -> String {
    let mut segments: Vec<&str> = Vec::with_capacity(ns_path.len() + 2);
    segments.push(prefix);
    segments.extend(ns_path.iter().map(Name::as_str));
    segments.push(name);
    segments.join(".")
}

/// The item itself plus its members, per definition kind.
fn collect_definition_candidates(
    db: &dyn baml_compiler2_ppir::Db,
    def: Definition<'_>,
    name: &Name,
    path: &str,
    out: &mut Vec<Candidate>,
) {
    use baml_compiler2_ast::ast::FunctionOrigin;

    let member = |name: &str, kind: &'static str, docstring: Option<&String>| Candidate {
        kind,
        leaf: name.to_string(),
        path: format!("{path}.{name}"),
        docstring: docstring.cloned(),
    };

    let item_docstring = match def {
        Definition::Class(loc) => {
            let class = item_data::class_data(db, loc);
            out.extend(
                class
                    .fields
                    .iter()
                    .map(|f| member(f.name.as_str(), "field", f.docstring.as_ref())),
            );
            for &method in &class.methods {
                let func = item_data::function_data(db, method);
                if !matches!(func.metadata.origin, FunctionOrigin::UserDefined) {
                    continue;
                }
                out.push(member(
                    func.name.as_str(),
                    "method",
                    func.docstring.as_ref(),
                ));
            }
            class.docstring.clone()
        }
        Definition::Enum(loc) => {
            let enum_data = item_data::enum_data(db, loc);
            out.extend(
                enum_data
                    .variants
                    .iter()
                    .map(|v| member(v.name.as_str(), "variant", v.docstring.as_ref())),
            );
            enum_data.docstring.clone()
        }
        Definition::Interface(loc) => {
            let iface = item_data::interface_data(db, loc);
            out.extend(
                iface
                    .fields
                    .iter()
                    .map(|f| member(f.name.as_str(), "field", f.docstring.as_ref())),
            );
            out.extend(
                iface
                    .associated_types
                    .iter()
                    .map(|a| member(a.name.as_str(), "associated type", None)),
            );
            // `methods` carries default and required methods alike (a
            // required method is a function with no body) — one uniform
            // enumeration, no double counting via the bodied-subset view.
            for &method in &iface.methods {
                let func = item_data::function_data(db, method);
                out.push(member(
                    func.name.as_str(),
                    "method",
                    func.docstring.as_ref(),
                ));
            }
            iface.docstring.clone()
        }
        Definition::Function(loc) => item_data::function_data(db, loc).docstring.clone(),
        Definition::TypeAlias(loc) => item_data::type_alias_data(db, loc).docstring.clone(),
        // Contributions without firewall docstring readers today; still
        // findable by name.
        Definition::TemplateString(_)
        | Definition::Client(_)
        | Definition::Test(_)
        | Definition::RetryPolicy(_)
        | Definition::Let(_) => None,
    };

    out.push(Candidate {
        kind: def_kind(def).as_str(),
        leaf: name.as_str().to_string(),
        path: path.to_string(),
        docstring: item_docstring,
    });
}

fn def_kind(def: Definition<'_>) -> DefinitionKind {
    match def {
        Definition::Class(_) => DefinitionKind::Class,
        Definition::Enum(_) => DefinitionKind::Enum,
        Definition::Interface(_) => DefinitionKind::Interface,
        Definition::TypeAlias(_) => DefinitionKind::TypeAlias,
        Definition::Function(_) => DefinitionKind::Function,
        Definition::TemplateString(_) => DefinitionKind::TemplateString,
        Definition::Client(_) => DefinitionKind::Client,
        Definition::Test(_) => DefinitionKind::Test,
        Definition::RetryPolicy(_) => DefinitionKind::RetryPolicy,
        Definition::Let(_) => DefinitionKind::Let,
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_hir::package::sole_workspace_package;

    use super::*;
    use crate::test_support::ProjectTest;

    const FIXTURE: &str = r#"
/// Reads the whole file at a path into a string.
function load_file(path: string) -> string {
    path
}

/// A zoned instant in time.
class ZonedDateTime {
    /// Seconds since the epoch.
    seconds int
}

enum Mood {
    /// Cheerful and bright.
    Happy
    Sad
}
"#;

    fn project() -> ProjectTest {
        let mut builder = ProjectTest::builder();
        builder.source("main.baml", FIXTURE);
        builder.build()
    }

    #[test]
    fn substring_search_matches_case_insensitively_with_containers() {
        let test = project();
        let hits = search_symbols(&test.db, &test.files, "seconds");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "seconds");
        assert_eq!(hits[0].container_name.as_deref(), Some("ZonedDateTime"));

        let upper = search_symbols(&test.db, &test.files, "ZONED");
        assert_eq!(upper.len(), 1);
        assert_eq!(upper[0].name, "ZonedDateTime");
    }

    #[test]
    fn empty_query_lists_every_symbol() {
        let test = project();
        let hits = search_symbols(&test.db, &test.files, "");
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        for expected in [
            "load_file",
            "ZonedDateTime",
            "seconds",
            "Mood",
            "Happy",
            "Sad",
        ] {
            assert!(names.contains(&expected), "missing {expected} in {names:?}");
        }
    }

    #[test]
    fn ranked_search_finds_symbols_through_their_prose() {
        let test = project();
        let packages = [sole_workspace_package(&test.db)];
        // "read a file" appears in no symbol name — only in LoadFile's
        // docstring.
        let hits = search_ranked(&test.db, &packages, "read a file", 10);
        assert!(
            hits.iter().any(|hit| hit.path == "root.load_file"),
            "docstring words must reach load_file, got: {hits:?}"
        );
        let load = hits
            .iter()
            .find(|hit| hit.path == "root.load_file")
            .unwrap_or_else(|| unreachable!("asserted above"));
        assert_eq!(
            load.summary.as_deref(),
            Some("Reads the whole file at a path into a string.")
        );
    }

    #[test]
    fn ranked_search_splits_camel_case_names() {
        let test = project();
        let packages = [sole_workspace_package(&test.db)];
        // Typed as one PascalCase word, found via the camelCase-split
        // haystack; and the plain word "zoned" reaches it too.
        for query in ["ZonedDateTime", "zoned"] {
            let hits = search_ranked(&test.db, &packages, query, 10);
            assert!(
                hits.first()
                    .is_some_and(|hit| hit.path == "root.ZonedDateTime"),
                "query {query:?} should lead with the type, got: {hits:?}"
            );
        }
    }

    #[test]
    fn ranked_search_ranks_names_above_prose_and_members_carry_paths() {
        let test = project();
        let packages = [sole_workspace_package(&test.db)];
        let hits = search_ranked(&test.db, &packages, "happy", 10);
        assert!(
            hits.first()
                .is_some_and(|hit| hit.path == "root.Mood.Happy"),
            "variant name match leads, got: {hits:?}"
        );

        // Companions synthesized from LoadFile ($parse etc.) must not
        // duplicate its docstring hits.
        let hits = search_ranked(&test.db, &packages, "read a file", 20);
        let load_hits = hits
            .iter()
            .filter(|hit| hit.path.contains("load_file"))
            .count();
        assert_eq!(load_hits, 1, "companions excluded, got: {hits:?}");
    }
}
