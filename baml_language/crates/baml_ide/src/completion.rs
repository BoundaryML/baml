//! Completions: what can be written at the cursor.
//!
//! rust-analyzer's shape, in two halves that never trade jobs.
//!
//! **Classification** ([`context`]) answers *what kind of position is this*.
//! It runs on a SPECULATIVE parse — the file with a marker identifier spliced
//! in at the cursor — because a completion position is almost always a parse
//! error in the real text (`items.` is not an expression, `-> ` is not a type),
//! and a tree parsed from the broken text mislabels the position. Splicing a
//! plausible identifier in is what makes the classification uniform instead
//! of a pile of look-left rules.
//!
//! **Providers** ([`members`], and the contexts that follow it) answer *what
//! goes here*, and they read the compiler's recorded facts about the REAL
//! file: inferred receiver types, declared members, the resolution ladder.
//! The speculative tree never decides what a name means — it only says where
//! the cursor is. Offsets before the cursor are identical in both texts,
//! which is what lets a classification hand a provider a real-file position.

mod context;
mod item;
mod members;

use baml_base::SourceFile;
pub use context::CompletionAnalysis;
pub use item::{Completion, CompletionInsert, CompletionKind, CompletionRelevance};
use text_size::TextSize;

/// Everything that can be written at `offset`, best first.
///
/// Regular function (not cached): the expensive parts (parsing, the semantic
/// index, inference, member enumeration) are Salsa-cached underneath, and the
/// speculative parse is one lex+parse of one file.
pub fn completions(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Vec<Completion> {
    let Some(context) = context::CompletionContext::new(db, file, offset) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    match context.analysis {
        CompletionAnalysis::Member { dot } => {
            members::complete_members(db, file, dot, &context, &mut items);
        }
        CompletionAnalysis::Unsupported => {}
    }
    items.sort_by(|a, b| {
        b.relevance
            .score()
            .cmp(&a.relevance.score())
            .then_with(|| a.label.cmp(&b.label))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::CursorTest;

    fn complete(test: &CursorTest) -> Vec<Completion> {
        completions(&test.db, test.cursor.file, test.cursor.offset)
    }

    fn labels(items: &[Completion]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn a_bare_dot_offers_the_receivers_members() {
        let test = CursorTest::new(
            r#"function f() -> int {
    let a = "hi";
    a.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        for expected in ["length", "trim", "to_upper_case"] {
            assert!(
                labels.contains(&expected),
                "string receiver should offer `{expected}`, got {labels:?}"
            );
        }
    }

    #[test]
    fn a_partial_member_replaces_only_what_was_typed() {
        let test = CursorTest::new(
            r#"function f() -> int {
    let a = "hi";
    let b = a.le<[CURSOR];
    0
}
"#,
        );
        let items = complete(&test);
        let length = items
            .iter()
            .find(|item| item.label == "length")
            .expect("a partially typed member still completes from the receiver");
        let text = test.cursor.file.text(&test.db);
        assert_eq!(
            &text[length.source_range], "le",
            "an accepted item replaces the fragment already typed, nothing more"
        );
    }

    #[test]
    fn a_class_receiver_offers_its_fields_and_methods() {
        let test = CursorTest::new(
            r#"class Point {
    x: int
    y: int

    /// The origin distance, squared.
    function norm2(self) -> int throws never { self.x * self.x + self.y * self.y }
}

function f(p: Point) -> int {
    p.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert!(labels.contains(&"x") && labels.contains(&"y"), "{labels:?}");

        let norm2 = items
            .iter()
            .find(|item| item.label == "norm2")
            .expect("the class method completes");
        assert_eq!(norm2.kind, CompletionKind::Method);
        assert_eq!(
            norm2.insert,
            CompletionInsert::Snippet("norm2($0)".to_string()),
            "a method inserts its call, with the cursor between the parentheses"
        );
        assert_eq!(
            norm2.documentation.as_deref(),
            Some("The origin distance, squared."),
            "the declaration's own docs travel with the item"
        );
        assert!(
            norm2
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("norm2(self) -> int")),
            "detail is the signature hover renders, got {:?}",
            norm2.detail
        );

        let x = items.iter().find(|item| item.label == "x").unwrap();
        assert_eq!(x.kind, CompletionKind::Field);
        assert_eq!(x.detail.as_deref(), Some("int"));
        assert_eq!(x.insert, CompletionInsert::Plain("x".to_string()));
    }

    #[test]
    fn a_bounded_type_variable_offers_the_members_its_bound_declares() {
        // The payoff of enumerating in the owner's param env: `T` has
        // members only because the function declared the bound.
        let test = CursorTest::new(
            r#"function biggest<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {
    a.<[CURSOR]
    a
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(
            labels.iter().any(|label| label == "lt"),
            "a `Compare`-bounded receiver offers the interface's members, got {labels:?}"
        );
    }

    #[test]
    fn an_inherent_member_sorts_above_one_reached_through_an_interface() {
        let test = CursorTest::new(
            r#"class Tag {
    name: string

    function label(self) -> string throws never { self.name }
}

function f(t: Tag) -> int {
    t.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let own = items
            .iter()
            .position(|item| item.label == "label")
            .expect("own method completes");
        let inherited = items
            .iter()
            .position(|item| item.label == "to_json")
            .or_else(|| items.iter().position(|item| item.label == "to_string"));
        if let Some(inherited) = inherited {
            assert!(
                own < inherited,
                "own members come first: {:?}",
                labels(&items)
            );
        }
    }

    #[test]
    fn a_position_no_provider_claims_offers_nothing() {
        let test = CursorTest::new(
            r#"function f() -> int {
    let a = 1;
    a<[CURSOR]
}
"#,
        );
        assert!(
            complete(&test).is_empty(),
            "value position is not classified yet, and an unclassified position guesses nothing"
        );
    }
}
