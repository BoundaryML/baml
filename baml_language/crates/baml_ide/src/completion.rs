//! Completions: what can be written at the cursor.
//!
//! rust-analyzer's shape, in three parts that never trade jobs.
//!
//! **Analysis** (`context`) answers *what kind of position is this, and
//! what does the qualifier before it mean*. It classifies on a SPECULATIVE
//! parse — the file with a marker identifier spliced in at the cursor —
//! because a completion position is almost always a parse error in the real
//! text, and it resolves the dot before the cursor ONCE, into a
//! `context::DotTarget`, against the real file's recorded facts.
//!
//! **Providers** (`members`, `values`, `args`, `record`) answer
//! *what goes here*: one match arm per analysis, each enumerating from a
//! compiler enumeration and filtering to what a reader can write at the
//! position. No provider re-derives the position it was handed.
//!
//! **Presentation** (the [`completions`] accumulator and `render`) answers
//! *how an offer looks*: insert text, kind, and relevance are decided in the
//! accumulator, detail and documentation in the renderer — each rule stated
//! once, holding for every provider at once.

mod args;
mod completions;
mod context;
mod declarations;
mod item;
mod members;
mod record;
mod render;
mod types;
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
    db: &dyn baml_compiler2_hir::Db,
    file: SourceFile,
    offset: TextSize,
) -> Vec<Completion> {
    let Some(context) = CompletionContext::new(db, file, offset) else {
        return Vec::new();
    };
    // The fragment already typed. The accumulator reads it to decide whether
    // the reader is reaching for a package's internals on purpose.
    let typed = &file.text(db)[context.source_range];
    let mut out = Completions::new(db, file, context.source_range, typed);
    match &context.analysis {
        CompletionAnalysis::Path {
            kind,
            qualifier: Some(target),
        } => {
            members::complete(db, target, *kind, &mut out);
        }
        CompletionAnalysis::Path {
            kind: PathKind::Expr,
            qualifier: None,
        } => {
            values::complete(db, file, offset, &mut out);
        }
        CompletionAnalysis::Path {
            kind: PathKind::Type,
            qualifier: None,
        } => {
            types::complete(db, file, offset, &mut out);
        }
        CompletionAnalysis::CallArgument { call } => {
            // A slot takes a named argument OR an expression, so it offers
            // both; relevance is what puts the callee's own labels first.
            args::complete(call, &mut out);
            values::complete(db, file, offset, &mut out);
        }
        CompletionAnalysis::RecordField { literal } => {
            record::complete(db, literal, &mut out);
        }
        CompletionAnalysis::Item { container } => {
            declarations::complete_items(*container, &mut out);
        }
        CompletionAnalysis::Attribute { position } => {
            declarations::complete_attributes(*position, &mut out);
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
        // `summarize@stream` and friends resolve, but no reader can write an
        // `@` in a name, so an enumeration of what to WRITE drops them.
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
            !labels.iter().any(|label| label.contains('@')),
            "no companion spelling is offerable: {:?}",
            labels
                .iter()
                .filter(|label| label.contains('@'))
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
    fn a_field_type_is_a_type_position() {
        let test = CursorTest::new("class Foo {\n    bar: <[CURSOR]\n}\n");
        let items = complete(&test);
        let labels = labels(&items);
        // The class itself is legal (self-referential fields are types),
        // and so are builtins and package roots; values are not offered.
        for expected in ["Foo", "int", "baml"] {
            assert!(
                labels.contains(&expected),
                "missing `{expected}`: {labels:?}"
            );
        }
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
    fn an_enum_type_offers_its_impl_methods() {
        // An enum is a concrete type like any other: it cannot carry an
        // in-body block, but in-body is not special — the methods its impls
        // provide are members, reached through the type like a class's.
        let test = CursorTest::new(
            r#"enum Status { Active Done }

interface Named {
    function label(self) -> string throws never
}

implement Named for Status {
    function label(self) -> string throws never {
        return "status"
    }
}

function f() -> int {
    let a = Status.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert!(
            labels.contains(&"Active") && labels.contains(&"label"),
            "an enum's members are its variants AND its impl-provided methods: {labels:?}"
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
        // BUG: `Self` should read as `int` on both rungs. `clamp` comes from
        // `baml.ops.Compare`'s defaults rather than `class Int`, so it is
        // declared in terms of `Self`, and the renderer has no receiver to
        // substitute: `render::member` calls `resolved_function_sig_parts(db,
        // function, None)`, and `MemberCandidate` carries no substituted
        // signature. Both halves the substitution needs are already known
        // where candidates are enumerated (the receiver and the realized
        // `MemberSource::Interface`), and `FnSigParts` already has the
        // `SigSlot::Resolved` variants the mounted-function path fills.
        // Pinned as rendered so the rung DIFFERENCE stays covered; restore
        // `int` with that fix.

        // The reader already wrote the receiver, so it is not a parameter
        // they still have to pass.
        assert_eq!(
            detail(&through_value, "clamp"),
            "function clamp(min: Self, max: Self) -> Self throws never"
        );
        // Through the type it is the first argument, and reads like one.
        assert_eq!(
            detail(&through_type, "clamp"),
            "function clamp(self, min: Self, max: Self) -> Self throws never"
        );
    }

    #[test]
    fn a_package_qualifier_hides_only_the_carriers_with_another_spelling() {
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
        // written. Listing the carrier teaches a spelling nobody uses.
        for carrier in [
            "Int",
            "Bigint",
            "Float",
            "String",
            "Bool",
            "Null",
            "Uint8Array",
        ] {
            assert!(
                !labels.contains(&carrier),
                "`{carrier}` is reached as its alias, not by its carrier path: {labels:?}"
            );
        }
        // The containers have no such alias: neither `int[].filled` nor
        // `map<string, int>.of` parses, so hiding the carrier would leave
        // their statics with no spelling at all.
        for carrier in ["Array", "Map"] {
            assert!(
                labels.contains(&carrier),
                "`baml.{carrier}` is the only handle on its statics: {labels:?}"
            );
        }
        assert!(
            labels.contains(&"iter") && labels.contains(&"Sortable"),
            "the namespace's own items and children still come back: {labels:?}"
        );
    }

    /// `reflect.Type`'s class name IS the builtin's canonical spelling
    /// (`TYPE_SYSTEM.md`), not a stand-in for one, so the carrier rule must
    /// not reach it — there is nothing else to write.
    /// `baml.media` declares nothing BUT carriers — `Image`, `Audio`,
    /// `Video`, `Pdf` are the classes `image`, `audio`, `video`, `pdf`
    /// denote — so hiding carriers empties that qualifier entirely. That is
    /// the rule working, not failing: the alias is what source writes, and
    /// it is offered in every position where a reader can write one, so
    /// nothing is stranded behind the empty list.
    #[test]
    fn a_namespace_of_nothing_but_carriers_offers_nothing() {
        const ALIASES: [&str; 4] = ["image", "audio", "video", "pdf"];

        let carrier_path =
            CursorTest::new("function f() -> int {\n    let a = baml.media.<[CURSOR]\n    0\n}\n");
        let under_media = complete(&carrier_path);
        assert!(
            labels(&under_media).is_empty(),
            "every name under `baml.media` is reached by its alias instead"
        );

        for source in [
            // A type position...
            "function f(a: <[CURSOR]) -> int throws never { 0 }\n",
            // ...and a value position, where the alias roots
            // `image.from_base64(..)`.
            "function f() -> int {\n    let a = <[CURSOR]\n    0\n}\n",
        ] {
            let test = CursorTest::new(source);
            let items = complete(&test);
            let offered = labels(&items);
            for alias in ALIASES {
                assert!(
                    offered.contains(&alias),
                    "`{alias}` is the spelling the reader needs: {offered:?}"
                );
            }
        }
    }

    #[test]
    fn a_self_spelled_builtin_is_offered_under_its_own_path() {
        for (source, expected) in [
            (
                "function f() -> int {\n    let a = reflect.<[CURSOR]\n    0\n}\n",
                "Type",
            ),
            (
                "function f(a: reflect.<[CURSOR]) -> int throws never { 0 }\n",
                "Type",
            ),
            (
                "function f() -> int {\n    let a = baml.future.<[CURSOR]\n    0\n}\n",
                "Future",
            ),
        ] {
            let test = CursorTest::new(source);
            let items = complete(&test);
            let labels = labels(&items);
            assert!(
                labels.contains(&expected),
                "`{expected}` has no spelling but its own path: {labels:?}"
            );
        }
    }

    #[test]
    fn value_position_offers_builtin_aliases_as_qualifiers() {
        let test = CursorTest::new("function f() -> int {\n    let a = <[CURSOR]\n    0\n}\n");
        let items = complete(&test);
        let int_item = items
            .iter()
            .find(|item| item.label == "int")
            .unwrap_or_else(|| unreachable!("`int` roots `int.max_value()` and is offered"));
        assert_eq!(int_item.kind, CompletionKind::BuiltinType);
    }

    // ── Type positions (C3) ─────────────────────────────────────────────────

    #[test]
    fn a_type_annotation_offers_types_not_values() {
        let test = CursorTest::new(
            r#"class Point { x: int }

function helper() -> int throws never { 0 }

function f() -> int {
    let seed = 3;
    let x: <[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        for expected in ["Point", "int", "string", "json", "baml"] {
            assert!(
                labels.contains(&expected),
                "missing `{expected}`: {labels:?}"
            );
        }
        // What resolves as a VALUE does not resolve as a type.
        for absent in ["helper", "seed", "let", "return"] {
            assert!(
                !labels.contains(&absent),
                "`{absent}` is not a type: {labels:?}"
            );
        }
    }

    #[test]
    fn a_match_arm_is_a_type_position() {
        // A pattern IS a type (BEP-015); this position used to offer value
        // completions — locals and functions the checker would reject.
        let test = CursorTest::new(
            r#"class Circle { r: int }

function f(x: int | Circle) -> int {
    let seed = 3;
    match (x) {
        <[CURSOR]
    }
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert!(
            labels.contains(&"Circle") && labels.contains(&"int"),
            "{labels:?}"
        );
        assert!(
            !labels.contains(&"seed"),
            "locals are not patterns: {labels:?}"
        );
    }

    #[test]
    fn a_generic_parameter_completes_in_type_positions() {
        let test = CursorTest::new(
            r#"function pick<Elem>(xs: Elem[], fallback: <[CURSOR]) -> Elem {
    fallback
}
"#,
        );
        let items = complete(&test);
        let elem = items
            .iter()
            .find(|item| item.label == "Elem")
            .unwrap_or_else(|| unreachable!("the enclosing function's parameter is in scope"));
        assert_eq!(elem.kind, CompletionKind::TypeParam);
        // Local-analogue ranking: the declared parameter outranks package
        // types and builtins.
        assert_eq!(items.first().map(|item| item.label.as_str()), Some("Elem"));
    }

    #[test]
    fn a_qualified_type_position_reaches_only_types() {
        let test = CursorTest::new(
            r#"function f() -> int {
    let x: baml.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        assert!(!items.is_empty());
        for item in &items {
            assert!(
                matches!(
                    item.kind,
                    CompletionKind::Class
                        | CompletionKind::Enum
                        | CompletionKind::Interface
                        | CompletionKind::TypeAlias
                        | CompletionKind::Package
                ),
                "`{}` ({:?}) is not writable as a type",
                item.label,
                item.kind
            );
        }
    }

    #[test]
    fn an_enum_qualifier_in_a_pattern_offers_its_variants() {
        let test = CursorTest::new(
            r#"enum Status { Active Done }

function f(s: Status) -> int {
    match (s) {
        Status.<[CURSOR]
    }
}
"#,
        );
        let items = complete(&test);
        assert_eq!(
            labels(&items),
            vec!["Active", "Done"],
            "variants and nothing else"
        );
        assert!(
            items
                .iter()
                .all(|item| item.kind == CompletionKind::EnumVariant)
        );
    }

    #[test]
    fn intrinsics_are_offered_in_type_positions() {
        // `void` is offered EVERYWHERE a type can be written (ruling
        // 2026-08-25, superseding return-slots-only): `spawn` of a void
        // function makes `Future<void, E>` real, so `void` flows into
        // generic arguments and a slot restriction has no clean boundary.
        let annot = CursorTest::new("function f() -> int {\n    let x: <[CURSOR]\n    0\n}\n");
        let items = complete(&annot);
        let labels = labels(&items);
        for intrinsic in ["void", "never", "unknown"] {
            assert!(
                labels.contains(&intrinsic),
                "missing `{intrinsic}`: {labels:?}"
            );
        }
    }

    #[test]
    fn a_throws_clause_offers_types() {
        let test = CursorTest::new(
            r#"class MyError { message: string }

function f() -> int throws <[CURSOR]
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert!(
            labels.contains(&"MyError") && labels.contains(&"baml"),
            "{labels:?}"
        );
    }

    // ── Item positions and attributes (C4) ──────────────────────────────────

    #[test]
    fn top_level_offers_declaration_keywords() {
        let test = CursorTest::new("<[CURSOR]\n\nclass Later {\n    x: int\n}\n");
        let items = complete(&test);
        let labels = labels(&items);
        for keyword in [
            "class",
            "function",
            "enum",
            "interface",
            "type",
            "test",
            "let",
        ] {
            assert!(labels.contains(&keyword), "missing `{keyword}`: {labels:?}");
        }
        // Expression keywords and names do not open a declaration.
        for absent in ["return", "await", "Later"] {
            assert!(
                !labels.contains(&absent),
                "`{absent}` is not an item: {labels:?}"
            );
        }
        let class = items.iter().find(|item| item.label == "class").unwrap();
        assert_eq!(class.kind, CompletionKind::Keyword);
        // Accepting a declaration writes its skeleton with tab stops; the
        // plain-text downgrade collapses to `class Name { }`-shaped text
        // for clients without snippet support.
        assert_eq!(
            class.insert,
            CompletionInsert::Snippet("class ${1:Name} {\n\t$0\n}".to_string())
        );
        let test_item = items.iter().find(|item| item.label == "test").unwrap();
        assert_eq!(
            test_item.insert,
            CompletionInsert::Snippet("test \"${1:name}\" {\n\t$0\n}".to_string())
        );
    }

    #[test]
    fn class_and_interface_bodies_offer_their_own_vocabulary() {
        let class = CursorTest::new("class C {\n    x: int\n    <[CURSOR]\n}\n");
        let items = complete(&class);
        let labels_class = labels(&items);
        assert!(
            labels_class.contains(&"function") && labels_class.contains(&"implements"),
            "{labels_class:?}"
        );
        assert!(
            !labels_class.contains(&"class"),
            "no nested classes: {labels_class:?}"
        );

        let iface = CursorTest::new("interface I {\n    f: int\n    <[CURSOR]\n}\n");
        let items = complete(&iface);
        let labels_iface = labels(&items);
        assert!(
            labels_iface.contains(&"function") && labels_iface.contains(&"type"),
            "associated types and methods: {labels_iface:?}"
        );
        // The interface method skeleton is the bodyless required form and
        // spells `throws` — interface signatures must declare one (E0170),
        // so the snippet teaches the rule.
        let function = items.iter().find(|item| item.label == "function").unwrap();
        assert_eq!(
            function.insert,
            CompletionInsert::Snippet(
                "function ${1:name}(${2:self}) -> $3 throws ${4:never}".to_string()
            )
        );
    }

    #[test]
    fn attribute_position_offers_the_compilers_names() {
        let test = CursorTest::new("class C {\n    x: int @<[CURSOR]\n}\n");
        let items = complete(&test);
        let labels = labels(&items);
        for name in [
            "alias",
            "description",
            "skip",
            "stream.done",
            "stream.must_exist",
        ] {
            assert!(labels.contains(&name), "missing `{name}`: {labels:?}");
        }
        assert!(
            items
                .iter()
                .all(|item| item.kind == CompletionKind::Attribute)
        );
    }

    #[test]
    fn an_attribute_position_offers_only_what_has_meaning_there() {
        // The menu is the schema table filtered to the position: the
        // streaming attributes belong to class fields and classes, `skip` to
        // members, and an interface, its fields, and a function take none.
        let cases: [(&str, &[&str]); 6] = [
            (
                "enum E {\n    A @<[CURSOR]\n}\n",
                &["alias", "description", "skip"],
            ),
            (
                "class C {\n    x: int\n    @@<[CURSOR]\n}\n",
                &["alias", "description", "stream.done"],
            ),
            (
                "enum E {\n    A\n    @@<[CURSOR]\n}\n",
                &["alias", "description"],
            ),
            ("interface I {\n    x: int @<[CURSOR]\n}\n", &[]),
            ("interface I {\n    @@<[CURSOR]\n}\n", &[]),
            ("@@<[CURSOR]\nfunction f() -> int {\n    1\n}\n", &[]),
        ];
        for (source, expected) in cases {
            let test = CursorTest::new(source);
            let items = complete(&test);
            let mut labels = labels(&items);
            labels.sort_unstable();
            assert_eq!(labels, expected, "in {source:?}");
        }
    }

    #[test]
    fn a_dotted_attribute_fragment_replaces_the_whole_name() {
        // `@stream.⎸` is ONE dotted attribute name: accepting `stream.done`
        // must replace `stream.` too, not splice `stream.stream.done`.
        let test = CursorTest::new("class C {\n    x: int @stream.<[CURSOR]\n}\n");
        let items = complete(&test);
        let done = items
            .iter()
            .find(|item| item.label == "stream.done")
            .unwrap_or_else(|| unreachable!("the dotted names are offered"));
        let text = test.cursor.file.text(&test.db);
        assert_eq!(&text[done.source_range], "stream.");
    }

    #[test]
    fn an_enum_body_offers_nothing() {
        // Variant names are the reader's own; there is nothing to offer.
        let test = CursorTest::new("enum E {\n    A\n    <[CURSOR]\n}\n");
        assert!(complete(&test).is_empty());
    }

    /// Until BAML has `public`/`private`, a leading `_` is how the stdlib
    /// marks a helper as its own business. The tests below are the rule's
    /// two halves: the stdlib's internals stay out of the list, and
    /// everything in the reader's own source — or asked for by name —
    /// stays in.
    #[test]
    fn the_stdlibs_internal_members_are_not_offered() {
        let test = CursorTest::new(
            r#"function f() -> int throws never {
    let t = reflect.Type.of<int>();
    t.<[CURSOR]
    0
}
"#,
        );
        let labels = labels(&complete(&test))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(!labels.is_empty(), "the receiver still offers its surface");
        assert!(
            !labels.iter().any(|label| label.starts_with('_')),
            "`reflect.Type`'s internals are the stdlib's business, got {labels:?}"
        );
    }

    #[test]
    fn a_typed_underscore_asks_for_the_internals() {
        let test = CursorTest::new(
            r#"function f() -> int throws never {
    let t = reflect.Type.of<int>();
    t._<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let internal = items
            .iter()
            .find(|item| item.label == "_to_string_impl")
            .unwrap_or_else(|| {
                unreachable!(
                    "a typed `_` brings the internals back, got {:?}",
                    labels(&items)
                )
            });
        // The `_` the reader typed is part of what an accepted item
        // replaces, or the insert would read `t.__to_string_impl`.
        assert_eq!(
            &test.cursor.file.text(&test.db)[internal.source_range],
            "_",
            "the accepted item replaces the typed fragment"
        );
    }

    #[test]
    fn the_stdlibs_internal_items_are_not_offered_under_its_qualifier() {
        let hidden = CursorTest::new(
            r#"function f() -> int throws never {
    baml.time.<[CURSOR]
    0
}
"#,
        );
        let hidden_labels = labels(&complete(&hidden))
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(hidden_labels.iter().any(|label| label == "Instant"));
        assert!(
            !hidden_labels.iter().any(|label| label.starts_with('_')),
            "`baml.time`'s helpers are the stdlib's own, got {hidden_labels:?}"
        );

        let asked = CursorTest::new(
            r#"function f() -> int throws never {
    baml.time._<[CURSOR]
    0
}
"#,
        );
        let asked_items = complete(&asked);
        let asked_labels = labels(&asked_items);
        assert!(
            asked_labels.contains(&"_tz_offset_at"),
            "a typed `_` reaches them, got {asked_labels:?}"
        );
    }

    #[test]
    fn a_type_qualifier_hides_the_stdlibs_internal_statics() {
        let test = CursorTest::new(
            r#"function f() -> int throws never {
    int.<[CURSOR]
    0
}
"#,
        );
        let items = complete(&test);
        let labels = labels(&items);
        assert!(labels.contains(&"max_value"), "got {labels:?}");
        assert!(
            !labels.iter().any(|label| label.starts_with('_')),
            "the UFCS rung reads the same convention, got {labels:?}"
        );
    }

    #[test]
    fn the_readers_own_internals_are_always_offered() {
        // The reader's own source, so the convention has nothing to hide:
        // a member through a dot, a top-level item, and a local.
        let test = CursorTest::new(
            r#"class Point {
    x: int

    function _norm(self) -> int throws never { self.x }
}

function f(p: Point) -> int throws never {
    p.<[CURSOR]
    0
}
"#,
        );
        let dotted = complete(&test);
        let dotted_labels = labels(&dotted);
        assert!(dotted_labels.contains(&"_norm"), "got {dotted_labels:?}");

        let bare = CursorTest::new(
            r#"function _helper() -> int throws never { 1 }

function f() -> int throws never {
    let _seen = 1;
    <[CURSOR]
    0
}
"#,
        );
        let bare_items = complete(&bare);
        let bare_labels = labels(&bare_items);
        assert!(
            bare_labels.contains(&"_helper"),
            "own item: {bare_labels:?}"
        );
        assert!(bare_labels.contains(&"_seen"), "own local: {bare_labels:?}");
    }
}
