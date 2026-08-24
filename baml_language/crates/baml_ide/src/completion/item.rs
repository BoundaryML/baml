//! The completion item: what an editor shows, and how it ranks.

use text_size::TextRange;

/// One completion.
///
/// The item carries the range it REPLACES rather than trusting the editor to
/// guess a word boundary: `@alias`, `baml.http`, and a bare `x` all end at
/// the cursor but start in different places, and only the classification
/// knows where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// What the list shows, and what the editor filters on.
    pub label: String,
    /// The range this item replaces when accepted.
    pub source_range: TextRange,
    pub insert: CompletionInsert,
    pub kind: CompletionKind,
    /// The right-hand column: a signature, a type, whatever names the shape.
    pub detail: Option<String>,
    /// The declaration's own documentation, verbatim.
    pub documentation: Option<String>,
    pub relevance: CompletionRelevance,
}

/// How the editor interprets the inserted text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompletionInsert {
    /// Literal text.
    Plain(String),
    /// LSP snippet syntax (`${1:name}`, `$0`). Only ever produced when the
    /// client said it understands snippets; the protocol layer downgrades.
    Snippet(String),
}

impl CompletionInsert {
    /// The text to insert when snippets are unavailable: a snippet's
    /// placeholders collapse away rather than being shown literally.
    pub fn plain_text(&self) -> String {
        match self {
            CompletionInsert::Plain(text) => text.clone(),
            CompletionInsert::Snippet(snippet) => strip_snippet(snippet),
        }
    }
}

/// The semantic kind of a completion, which the protocol layer maps to
/// `lsp_types::CompletionItemKind` and the CLI to its own presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Field,
    Method,
    Function,
    Local,
    Parameter,
    Class,
    Enum,
    EnumVariant,
    Interface,
    TypeAlias,
    Client,
    RetryPolicy,
    /// A dependency package that roots a qualified path (`baml.http.fetch`).
    Package,
    Keyword,
    /// An item kind with no completion-specific presentation.
    Other,
}

/// Why an item should rank where it does.
///
/// Deliberately a set of FACTS, not a number: the score is derived, so a new
/// signal is added by naming the fact once and weighting it once, and a
/// ranking question always has an answer in terms of the program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about one item, deliberately additive: a member \
              can be inherent AND from the reader's own package, and the score \
              below is derived from all of them. Collapsing them into an enum \
              would force a ranking decision into the place that observes the \
              fact instead of the place that weighs it."
)]
pub struct CompletionRelevance {
    /// The receiver's own type declares it, rather than an interface it
    /// reaches. Own members are what a reader means by "its members".
    pub is_inherent: bool,
    /// A local or parameter: what the reader just wrote is what the reader
    /// most likely means.
    pub is_local: bool,
    /// A named parameter offered in the call it belongs to — the most
    /// specific thing an argument slot can be.
    pub is_parameter: bool,
    /// Declared in the reader's own package rather than a dependency. The
    /// stdlib's root namespace holds ~150 companion classes and interfaces
    /// that a bare name CAN reach; they are not what a reader reaching for
    /// their own `Point` means.
    pub is_own_package: bool,
}

impl CompletionRelevance {
    /// Higher sorts first. Kept small and total so ties break on the label,
    /// which keeps the list stable between keystrokes.
    ///
    /// The weights say what the reader means: in an argument slot the
    /// callee's own parameter names beat anything else, then things the
    /// reader wrote nearby, then things the receiver itself declares.
    pub fn score(&self) -> u32 {
        u32::from(self.is_parameter) * 8
            + u32::from(self.is_local) * 4
            + u32::from(self.is_own_package) * 2
            + u32::from(self.is_inherent)
    }
}

/// Drop snippet placeholders, keeping their default text: `${1:name}` becomes
/// `name`, and a bare tab stop like `$0` disappears.
fn strip_snippet(snippet: &str) -> String {
    let mut out = String::with_capacity(snippet.len());
    let mut rest = snippet;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        rest = &rest[dollar + 1..];
        if let Some(body) = rest.strip_prefix('{') {
            let Some(close) = body.find('}') else {
                // Unbalanced: nothing sensible to strip, keep it literal.
                out.push('$');
                out.push('{');
                rest = body;
                continue;
            };
            let placeholder = &body[..close];
            if let Some((_, default)) = placeholder.split_once(':') {
                out.push_str(default);
            }
            rest = &body[close + 1..];
        } else {
            // `$0`, `$1`, … — a bare tab stop with no default text.
            rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snippet_collapses_to_its_defaults_for_clients_without_snippet_support() {
        assert_eq!(
            CompletionInsert::Snippet("map(${1:f})$0".to_string()).plain_text(),
            "map(f)"
        );
        assert_eq!(
            CompletionInsert::Snippet("length()".to_string()).plain_text(),
            "length()"
        );
    }

    #[test]
    fn relevance_ranks_parameters_over_locals_over_inherited_members() {
        let parameter = CompletionRelevance {
            is_parameter: true,
            ..CompletionRelevance::default()
        };
        let local = CompletionRelevance {
            is_local: true,
            ..CompletionRelevance::default()
        };
        let own = CompletionRelevance {
            is_own_package: true,
            ..CompletionRelevance::default()
        };
        let inherent = CompletionRelevance {
            is_inherent: true,
            ..CompletionRelevance::default()
        };
        assert!(parameter.score() > local.score());
        assert!(local.score() > own.score());
        assert!(own.score() > inherent.score());
        assert!(inherent.score() > CompletionRelevance::default().score());
    }
}
