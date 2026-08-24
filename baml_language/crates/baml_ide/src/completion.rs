//! Completions: what can be written at the cursor.
//!
//! rust-analyzer's shape, in three parts that never trade jobs.
//!
//! **Analysis** ([`context`]) answers *what kind of position is this, and
//! what does the qualifier before it mean*. It classifies on a SPECULATIVE
//! parse — the file with a marker identifier spliced in at the cursor —
//! because a completion position is almost always a parse error in the real
//! text, and it resolves the dot before the cursor ONCE, into a
//! [`context::DotTarget`], against the real file's recorded facts.
//!
//! **Providers** ([`members`], [`values`], [`args`], [`record`]) answer
//! *what goes here*: one match arm per analysis, each enumerating from a
//! compiler enumeration and filtering to what a reader can write at the
//! position. No provider re-derives the position it was handed.
//!
//! **Presentation** (the [`completions`] accumulator and [`render`]) answers
//! *how an offer looks*: insert text, kind, and relevance are decided in the
//! accumulator, detail and documentation in the renderer — each rule stated
//! once, holding for every provider at once.

mod args;
mod completions;
mod context;
mod item;
mod members;
mod record;
mod render;
mod values;

use baml_base::SourceFile;
pub use item::{Completion, CompletionInsert, CompletionKind, CompletionRelevance};
use text_size::TextSize;

use self::{
    completions::Completions,
    context::{CompletionAnalysis, CompletionContext, PathKind},
};

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
    let Some(context) = CompletionContext::new(db, file, offset) else {
        return Vec::new();
    };
    let mut out = Completions::new(context.source_range);
    match &context.analysis {
        CompletionAnalysis::Path {
            kind: PathKind::Expr,
            qualifier: Some(target),
        } => {
            members::complete(db, file, target, &mut out);
        }
        CompletionAnalysis::Path {
            kind: PathKind::Expr,
            qualifier: None,
        } => {
            values::complete(db, file, offset, &mut out);
        }
        CompletionAnalysis::CallArgument { call } => {
            // A slot takes a named argument OR an expression, so it offers
            // both; relevance is what puts the callee's own labels first.
            args::complete(db, file, call, &mut out);
            values::complete(db, file, offset, &mut out);
        }
        CompletionAnalysis::RecordField { literal } => {
            record::complete(db, file, literal, &mut out);
        }
        CompletionAnalysis::Unsupported => {}
    }
    out.into_sorted()
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
                .is_some_and(|detail| detail.contains("norm2() -> int")),
            "detail is the signature hover renders, minus the receiver the \
             reader already wrote; got {:?}",
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
    fn value_position_offers_locals_items_packages_and_keywords() {
        let test = CursorTest::new(
            r#"function helper(n: int) -> int throws never { n }

function f(seed: int) -> int throws never {
    let total = seed;
    to<[CURSOR]
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        for expected in ["total", "seed", "helper", "baml", "let", "match"] {
            assert!(
                labels.contains(&expected),
                "value position should offer `{expected}`, got {labels:?}"
            );
        }
        let helper = items.iter().find(|item| item.label == "helper").unwrap();
        assert_eq!(helper.kind, CompletionKind::Function);
        assert_eq!(
            helper.insert,
            CompletionInsert::Snippet("helper($0)".to_string())
        );
        assert!(
            helper
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("helper(n: int) -> int")),
            "got {:?}",
            helper.detail
        );
        assert_eq!(
            items.iter().find(|item| item.label == "baml").unwrap().kind,
            CompletionKind::Package
        );
    }

    #[test]
    fn a_synthesized_companion_is_never_offered() {
        // `Summarize$stream` and friends resolve, but no reader can write a
        // `$` in a name, so an enumeration of what to WRITE drops them.
        let test = CursorTest::new(
            r#"function summarize(input: string) -> string {
    client: "openai/gpt-4o"
    prompt: `Summarize ${input}`
}

function f() -> int throws never {
    <[CURSOR]
    0
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(
            labels.iter().any(|label| label == "summarize"),
            "the function itself completes: {labels:?}"
        );
        assert!(
            !labels.iter().any(|label| label.contains('$')),
            "no companion spelling is offerable: {:?}",
            labels
                .iter()
                .filter(|label| label.contains('$'))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_shadowed_name_is_offered_once_as_the_binding_that_wins() {
        let test = CursorTest::new(
            r#"function f(x: int) -> int throws never {
    let x = 2;
    <[CURSOR]
}
"#,
        );
        let items = complete(&test);
        assert_eq!(
            items.iter().filter(|item| item.label == "x").count(),
            1,
            "the inner `let x` hides the parameter, so `x` is offered once"
        );
    }

    #[test]
    fn a_local_outranks_a_top_level_item() {
        let test = CursorTest::new(
            r#"function alpha() -> int throws never { 1 }

function f() -> int throws never {
    let beta = 1;
    <[CURSOR]
}
"#,
        );
        let items = complete(&test);
        let local = items.iter().position(|item| item.label == "beta").unwrap();
        let item = items.iter().position(|item| item.label == "alpha").unwrap();
        assert!(local < item, "locals first: {:?}", labels(&items));
    }

    #[test]
    fn an_argument_slot_offers_the_callees_named_parameters_first() {
        let test = CursorTest::new(
            r#"function search(query: string, limit: int = 10, strict: bool = false) -> int throws never {
    limit
}

function f() -> int throws never {
    search("cats", <[CURSOR])
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert_eq!(
            &labels[..2],
            &["limit", "strict"],
            "the callee's kwargs lead the slot, got {labels:?}"
        );
        let limit = items.iter().find(|item| item.label == "limit").unwrap();
        assert_eq!(limit.kind, CompletionKind::Parameter);
        assert_eq!(
            limit.insert,
            CompletionInsert::Plain("limit = ".to_string()),
            "a named argument inserts its `=`, ready for the value"
        );
        assert_eq!(limit.detail.as_deref(), Some("int"));
        assert!(
            labels.contains(&"search"),
            "a slot still takes an expression: {labels:?}"
        );
    }

    #[test]
    fn an_argument_slot_hides_a_label_the_call_already_wrote() {
        let test = CursorTest::new(
            r#"function search(query: string, limit: int = 10, strict: bool = false) -> int throws never {
    limit
}

function f() -> int throws never {
    search("cats", limit = 5, <[CURSOR])
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(
            !labels.iter().any(|label| label == "limit"),
            "a label written once is spoken for: {labels:?}"
        );
        assert!(labels.iter().any(|label| label == "strict"), "{labels:?}");
    }

    #[test]
    fn the_value_of_a_named_argument_is_an_expression_not_another_label() {
        let test = CursorTest::new(
            r#"function search(query: string, limit: int = 10) -> int throws never { limit }

function f() -> int throws never {
    let cap = 3;
    search("cats", limit = <[CURSOR])
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(labels.iter().any(|label| label == "cap"), "{labels:?}");
        assert!(
            !labels.iter().any(|label| label == "limit"),
            "past the `=` the slot is a value: {labels:?}"
        );
    }

    #[test]
    fn an_object_literal_offers_the_classes_unwritten_fields() {
        let test = CursorTest::new(
            r#"class Point {
    /// Horizontal position.
    x: int
    y: int
}

function f() -> int throws never {
    let p = Point { x: 1, <[CURSOR] };
    p.x
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert_eq!(labels, vec!["y"], "`x` is already written");
        let y = &items[0];
        assert_eq!(y.kind, CompletionKind::Field);
        assert_eq!(y.insert, CompletionInsert::Plain("y: ".to_string()));
        assert_eq!(y.detail.as_deref(), Some("int"));
    }

    #[test]
    fn an_object_literal_inside_an_argument_completes_its_own_fields() {
        let test = CursorTest::new(
            r#"class Point {
    x: int
    y: int
}

function take(p: Point, tag: string = "t") -> int throws never { p.x }

function f() -> int throws never {
    take(Point { <[CURSOR] })
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec!["x", "y"],
            "the innermost node wins — this is a field slot, not the call's"
        );
    }

    #[test]
    fn a_package_qualifier_offers_its_items_and_child_namespaces() {
        let test = CursorTest::new(
            r#"function f() -> int throws never {
    baml.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(!items.is_empty(), "`baml.` reaches the package's surface");
        assert!(
            labels.iter().any(|label| label == "http"),
            "child namespaces qualify further: {:?}",
            &labels[..labels.len().min(12)]
        );
        let http = items.iter().find(|item| item.label == "http").unwrap();
        assert_eq!(http.kind, CompletionKind::Package);
        assert!(
            !labels.iter().any(|label| label.contains('$')),
            "no companion spellings"
        );
    }

    #[test]
    fn a_namespace_qualifier_narrows_to_that_namespace() {
        let test = CursorTest::new(
            r#"function f() -> int throws never {
    baml.json.<[CURSOR]
    0
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(!labels.is_empty(), "`baml.json.` reaches the namespace");
        assert!(
            !labels.iter().any(|label| label == "http"),
            "a sibling namespace is not in it: {labels:?}"
        );
    }

    #[test]
    fn a_dependencys_items_are_offered_under_their_qualifier_not_bare() {
        // The language's rule: a bare name reaches what the file's own
        // namespace declares, and everything else is written qualified. The
        // list says the same thing — the package name, and the items one
        // qualifier along.
        let test = CursorTest::new(
            r#"class Point {
    x: int
}

function f() -> int throws never {
    <[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let bare: Vec<String> = labels(&items).into_iter().map(str::to_string).collect();
        assert!(
            bare.iter().any(|label| label == "Point"),
            "own items stay: {bare:?}"
        );
        assert!(
            bare.iter().any(|label| label == "baml"),
            "the qualifier is how you reach the rest: {bare:?}"
        );
        for stdlib in ["ToJson", "FromJson", "TaggedString", "AnyClass", "Summable"] {
            assert!(
                !bare.iter().any(|label| label == stdlib),
                "`{stdlib}` belongs under `baml.`, got {bare:?}"
            );
        }

        // And it is still there, one qualifier along.
        let qualified = CursorTest::new(
            r#"function f() -> int throws never {
    baml.<[CURSOR]
    0
}
"#,
        );
        let qualified_items = complete(&qualified);
        assert!(
            labels(&qualified_items).contains(&"ToJson"),
            "`baml.` reaches it"
        );
    }

    #[test]
    fn prose_is_not_code() {
        // Typing in a comment or a string is writing prose; a suggestion
        // list there is noise, and worse, it is noise that accepts on Tab.
        for (what, source) in [
            (
                "line comment",
                "function f() -> int throws never {\n    // note <[CURSOR]\n    0\n}\n",
            ),
            (
                "string literal",
                "function f() -> string throws never {\n    \"hello <[CURSOR]\"\n}\n",
            ),
            (
                "backtick prose",
                "function chat(msg: string) -> string {\n    client: \"openai/gpt-4o\"\n                     prompt: `hello <[CURSOR] world`\n}\n",
            ),
        ] {
            let test = CursorTest::new(source);
            assert!(
                complete(&test).is_empty(),
                "{what} should stay quiet, got {:?}",
                labels(&complete(&test))
            );
        }
    }

    #[test]
    fn a_prompt_interpolation_is_code() {
        // The other half of the same rule: `${…}` inside a template IS an
        // expression, and completes like one — including its members.
        let values = CursorTest::new(
            "function chat(msg: string) -> string {\n    client: \"openai/gpt-4o\"\n                 prompt: `hi ${<[CURSOR]}`\n}\n",
        );
        assert!(
            labels(&complete(&values)).contains(&"msg"),
            "the prompt's own parameter completes inside the hole"
        );

        let members = CursorTest::new(
            "function chat(msg: string) -> string {\n    client: \"openai/gpt-4o\"\n                 prompt: `hi ${msg.<[CURSOR]}`\n}\n",
        );
        assert!(
            labels(&complete(&members)).contains(&"to_upper_case"),
            "and so do its members"
        );
    }

    #[test]
    fn a_member_access_written_across_lines_still_finds_its_receiver() {
        let test = CursorTest::new(
            r#"function f(s: string) -> int throws never {
    s
        .<[CURSOR]
    0
}
"#,
        );
        assert!(
            labels(&complete(&test)).contains(&"trim"),
            "only trivia separates the receiver from the dot"
        );
    }

    #[test]
    fn an_optional_chain_offers_the_payloads_members() {
        let optional = CursorTest::new(
            r#"function f(a: string?) -> int throws never {
    a?.<[CURSOR]
    0
}
"#,
        );
        assert!(
            labels(&complete(&optional)).contains(&"trim"),
            "`?.` reads a member of the non-null payload"
        );

        // And the plain dot does NOT: an optional has no members of its own,
        // which is exactly the diagnostic the reader needs to see.
        let plain = CursorTest::new(
            r#"function f(a: string?) -> int throws never {
    a.<[CURSOR]
    0
}
"#,
        );
        assert!(complete(&plain).is_empty());
    }

    #[test]
    fn a_position_no_provider_claims_offers_nothing() {
        // A type annotation is C3's business; until then it stays silent
        // rather than offering value completions that cannot go there.
        let test = CursorTest::new("class Foo {\n    bar: <[CURSOR]\n}\n");
        assert!(complete(&test).is_empty(), "{:?}", labels(&complete(&test)));
    }

    // ── The type rung: statics, UFCS, and what a type is NOT ────────────────

    #[test]
    fn a_type_qualifier_offers_statics_and_instance_methods() {
        let test = CursorTest::new(
            r#"function f() -> int {
    let a = baml.iter.Range.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        // `new` has no `self`: it is reached HERE and nowhere else.
        assert!(
            labels.contains(&"new"),
            "statics belong to the type: {labels:?}"
        );
        // `next` has one: UFCS makes the receiver an argument, so the type
        // reaches it too (`Range.next(r)` is `r.next()`).
        assert!(
            labels.contains(&"next"),
            "UFCS reaches instance methods: {labels:?}"
        );
    }

    #[test]
    fn a_type_qualifier_offers_no_fields() {
        let test = CursorTest::new(
            r#"class Point {
    x: int

    function origin() -> int throws never { 0 }
    function norm(self) -> int throws never { self.x }
}

function f() -> int {
    let a = Point.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        // UFCS turns a `self` receiver into an argument; a field has no
        // argument to become, and `Point.x` is `unresolved name`.
        assert!(
            !labels.contains(&"x"),
            "a field is not a member of the type: {labels:?}"
        );
        assert!(
            labels.contains(&"origin") && labels.contains(&"norm"),
            "{labels:?}"
        );
    }

    #[test]
    fn an_enum_type_offers_its_variants() {
        let test = CursorTest::new(
            r#"enum Status { Active Done }

function f() -> int {
    let a = Status.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        assert_eq!(labels(&items), vec!["Active", "Done"]);
        assert!(
            items
                .iter()
                .all(|item| item.kind == CompletionKind::EnumVariant),
            "a variant completes as a variant, not a field"
        );
    }

    #[test]
    fn a_value_receiver_never_offers_a_static() {
        let test = CursorTest::new(
            r#"function f(n: int) -> int {
    let a = n.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert!(
            labels.contains(&"abs"),
            "instance methods are reached here: {labels:?}"
        );
        // The checker rejects `n.max_value()` ("no `self` receiver, so it
        // cannot be called on a value"), so offering it would propose a
        // call that cannot compile.
        assert!(
            !labels.contains(&"max_value"),
            "a static is reached through the type, not a value: {labels:?}"
        );
    }

    #[test]
    fn the_two_rungs_render_the_self_receiver_differently() {
        let through_value = CursorTest::new(
            r#"function f(n: int) -> int {
    let a = n.<[CURSOR]
    0
}
"#,
        );
        let through_type = CursorTest::new(
            r#"function f() -> int {
    let a = int.<[CURSOR]
    0
}
"#,
        );
        let detail = |test: &CursorTest, name: &str| {
            complete(test)
                .into_iter()
                .find(|item| item.label == name)
                .and_then(|item| item.detail)
                .unwrap_or_else(|| unreachable!("`{name}` is offered on an int"))
        };
        // The reader already wrote the receiver, so it is not a parameter
        // they still have to pass.
        assert_eq!(
            detail(&through_value, "clamp"),
            "function clamp(min: int, max: int) -> int throws never"
        );
        // Through the type it is the first argument, and reads like one.
        assert_eq!(
            detail(&through_type, "clamp"),
            "function clamp(self, min: int, max: int) -> int throws never"
        );
    }

    #[test]
    fn a_package_qualifier_hides_the_companion_carriers() {
        let test = CursorTest::new(
            r#"function f() -> int {
    let a = baml.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        // `baml.Int` is where `int`'s methods live, and `int` is how it is
        // written; the same for `baml.Array` (`T[]`) and `baml.Map`
        // (`map<K, V>`). Listing the carrier teaches a spelling nobody uses.
        for carrier in ["Int", "String", "Array", "Map", "Bool", "TypeValue"] {
            assert!(
                !labels.contains(&carrier),
                "`baml.{carrier}` is a companion carrier, not a name to write: {labels:?}"
            );
        }
        assert!(
            labels.contains(&"iter") && labels.contains(&"Comparable"),
            "the namespace's own items and children still come back: {labels:?}"
        );
    }
}
