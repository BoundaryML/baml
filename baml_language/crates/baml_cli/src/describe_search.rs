//! Full-text search over the surface, for `baml describe --search`.
//!
//! `describe` resolves names: given `exec` it finds `baml.sys.exec`, and given
//! a near miss it suggests one. What it cannot do is answer a question phrased
//! as a question — `baml describe subprocess` reports "no symbol found",
//! because no symbol is called that. Someone who knows what they want to *do*
//! and not what it is *called* has nothing to type.
//!
//! This searches docstrings as well as names, which is where that knowledge
//! lives: `baml.sys.exec` never says "subprocess", but it does say "Runs
//! `program` with the given `args`".
//!
//! Matching is on whole words, not substrings. Substrings are how a search
//! becomes noise — `run` is inside `trunc`, so a substring search for "run a
//! subprocess" leads with `float.trunc` and `float.itrunc` while `exec` does
//! not appear at all.

use baml_project::ProjectDatabase;
use baml_surface::export::{ItemDetail, PackageExport};

/// One result: what it is, how to name it, and enough of what it does to decide
/// whether to look closer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Hit {
    pub id: String,
    pub kind: &'static str,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub score: u32,
}

/// The words a query is looking for: lowercased, and long enough to mean
/// something. One- and two-character tokens match nearly everything.
fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 2)
        .map(str::to_ascii_lowercase)
        .take(8)
        .collect()
}

/// A name or docstring reduced to whole words, space-delimited at both ends so
/// `" token "` asks for an exact word and `" token"` for a word starting with
/// one.
///
/// Splits on camelCase as well as on punctuation, so a query written in one
/// language's casing finds a name written in the other's.
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
/// The symbol's own name counts for most, its docstring next, and the path it
/// hangs under least. Weighting the whole path equally made every member of a
/// matching type rank with it: a search for "run" returned `RunReport.message`
/// and `RunReport.duration_ms` above anything that runs something, because both
/// live under a name containing the word.
///
/// A prefix still scores, so `run` finds "Runs" — English inflection is not
/// worth a stemmer here, but ignoring it entirely would lose the most natural
/// way to ask.
fn score(leaf: &str, owner: &str, docs: &str, tokens: &[String]) -> u32 {
    let mut total = 0;
    for token in tokens {
        let exact = format!(" {token} ");
        let prefix = format!(" {token}");
        if leaf.contains(&exact) {
            total += 20;
        } else if leaf.contains(&prefix) {
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

/// The first line of a docstring, which is where the summary is by convention.
fn summarize(docstring: Option<&String>) -> Option<String> {
    let text = docstring?.lines().next()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// One thing that can be found: an item, or a member of one.
struct Candidate {
    id: String,
    kind: &'static str,
    /// The symbol's own name, which is what a query is usually reaching for.
    leaf: String,
    /// The full dotted path, shown to the reader and scored weakly.
    path: String,
    docstring: Option<String>,
}

/// Everything in a package that a search can land on.
///
/// The match over `ItemDetail` is exhaustive on purpose: a kind that gains
/// members should fail to compile here rather than quietly become unfindable.
fn candidates(export: &PackageExport) -> Vec<Candidate> {
    let mut out = Vec::new();
    for item in &export.items {
        // Auto-derived companions (`Foo$stream`) carry the docstring of the
        // type they shadow, so every one of them is a duplicate hit for
        // whatever its original matched.
        if item.synthetic {
            continue;
        }
        let mut path = item.namespace.join(".");
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(&item.name);

        let member =
            |name: &str, id: &str, kind: &'static str, docstring: Option<&String>| Candidate {
                id: id.to_string(),
                kind,
                leaf: name.to_string(),
                path: format!("{path}.{name}"),
                docstring: docstring.cloned(),
            };
        match &item.detail {
            ItemDetail::Class {
                fields, methods, ..
            } => {
                out.extend(
                    fields
                        .iter()
                        .map(|f| member(&f.name, &f.id, "field", f.docstring.as_ref())),
                );
                out.extend(
                    methods
                        .iter()
                        .map(|m| member(&m.name, &m.id, "method", m.docstring.as_ref())),
                );
            }
            ItemDetail::Enum { variants, .. } => {
                out.extend(
                    variants
                        .iter()
                        .map(|v| member(&v.name, &v.id, "variant", v.docstring.as_ref())),
                );
            }
            ItemDetail::Interface {
                fields,
                assoc_types,
                required_methods,
                default_methods,
                ..
            } => {
                out.extend(
                    fields
                        .iter()
                        .map(|f| member(&f.name, &f.id, "field", f.docstring.as_ref())),
                );
                out.extend(
                    assoc_types
                        .iter()
                        .map(|a| member(&a.name, &a.id, "associated type", None)),
                );
                out.extend(
                    required_methods
                        .iter()
                        .map(|m| member(&m.name, &m.id, "method", m.docstring.as_ref())),
                );
                out.extend(
                    default_methods
                        .iter()
                        .map(|m| member(&m.name, &m.id, "method", m.docstring.as_ref())),
                );
            }
            ItemDetail::TypeAlias { .. } | ItemDetail::Function { .. } | ItemDetail::Plain {} => {}
        }

        out.push(Candidate {
            id: item.id.clone(),
            kind: item.kind.as_str(),
            leaf: item.name.clone(),
            path,
            docstring: item.docstring.clone(),
        });
    }

    // Methods reached through an impl are not on the item at all — they hang
    // off the block. Skipping them made a whole class of API unfindable:
    // sorting, comparison, iteration and every operator arrive this way, so a
    // search for "sort an array" could not reach `T[].sort`.
    for block in &export.impls {
        for method in &block.methods {
            out.push(Candidate {
                id: method.id.clone(),
                kind: "method",
                leaf: method.name.clone(),
                // As the reader would write the call, not as the impl is
                // declared: `T[].sort`, not `baml.Sortable for T[]::sort`.
                path: format!("{}.{}", block.for_ty.display, method.name),
                docstring: method.docstring.clone(),
            });
        }
    }
    out
}

/// Search every package the project can see, best first.
pub fn search(db: &ProjectDatabase, query: &str, limit: usize) -> Vec<Hit> {
    let tokens = query_tokens(query);
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut names: Vec<String> = baml_lsp2_actions::non_user_package_names(db)
        .into_iter()
        .collect();
    names.push("user".to_string());
    // A stable walk order, so equal-scoring hits from different packages come
    // out the same way on every run.
    names.sort();

    let mut hits: Vec<Hit> = Vec::new();
    for package_name in names {
        let Some(baml_surface::Resolved::Package(package)) =
            baml_surface::resolve(db, &package_name)
        else {
            continue;
        };
        for candidate in candidates(&baml_surface::export_package(db, package)) {
            let docs = candidate.docstring.as_deref().unwrap_or("");
            let owner = candidate
                .path
                .strip_suffix(&candidate.leaf)
                .unwrap_or(&candidate.path)
                .to_string();
            let score = score(
                &haystack(&candidate.leaf),
                &haystack(&owner),
                &haystack(docs),
                &tokens,
            );
            if score > 0 {
                hits.push(Hit {
                    id: candidate.id,
                    kind: candidate.kind,
                    path: candidate.path,
                    summary: summarize(candidate.docstring.as_ref()),
                    score,
                });
            }
        }
    }

    // Best first, then by path, so equal scores come out in a stable order
    // rather than in whichever order the packages were walked.
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_words_only() {
        // The bug this scoring exists to prevent: `run` is a substring of
        // `trunc`, and a substring search buries the answer under arithmetic.
        let tokens = query_tokens("run a subprocess");
        assert_eq!(tokens, ["run", "subprocess"]);
        assert_eq!(
            score(
                &haystack("trunc"),
                &haystack("float"),
                &haystack(""),
                &tokens
            ),
            0
        );
        assert!(
            score(
                &haystack("exec"),
                &haystack("baml.sys"),
                &haystack("Runs `program`"),
                &tokens
            ) > 0
        );
    }

    #[test]
    fn a_name_outweighs_prose() {
        let tokens = query_tokens("exec");
        let named = score(
            &haystack("exec"),
            &haystack("baml.sys"),
            &haystack(""),
            &tokens,
        );
        let mentioned = score(
            &haystack("shell"),
            &haystack("baml.sys"),
            &haystack("exec is similar"),
            &tokens,
        );
        assert!(named > mentioned, "{named} should beat {mentioned}");

        // A member must not inherit its owner's relevance: every field of
        // `RunReport` outranked everything that actually runs something.
        let run = query_tokens("run");
        let owner_only = score(
            &haystack("duration_ms"),
            &haystack("RunReport"),
            &haystack(""),
            &run,
        );
        let by_name = score(
            &haystack("run_all"),
            &haystack("TestRegistry"),
            &haystack(""),
            &run,
        );
        assert!(by_name > owner_only, "{by_name} should beat {owner_only}");
    }

    #[test]
    fn camel_case_splits_into_words() {
        // A query typed in snake_case has to find a name written in camelCase,
        // which is the whole reason names are split rather than compared.
        assert_eq!(haystack("toUpperCase"), " to upper case ");
        assert_eq!(haystack("baml.sys.exec"), " baml sys exec ");
        assert_eq!(haystack("read_dir"), " read dir ");
    }

    #[test]
    fn short_tokens_are_dropped() {
        // `a` and `of` match nearly every docstring in the standard library.
        assert_eq!(query_tokens("a list of the"), ["list", "the"]);
        assert!(query_tokens("").is_empty());
    }
}
