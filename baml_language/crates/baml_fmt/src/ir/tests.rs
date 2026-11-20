use bumpalo::Bump;

use super::{Doc, Docs, SuffixWidth};
use crate::FormatOptions;

fn print(root: Doc<'_>, line_width: usize) -> String {
    root.print(FormatOptions {
        line_width,
        indent_width: 4,
    })
}

/// `name(a, b, c)` as a group that breaks one argument per line, with a
/// trailing comma only when broken — the canonical call layout.
fn call<'a>(d: Docs<'a>, name: &str, args: &[&'a str]) -> Doc<'a> {
    let open = d.copy_text(&format!("{name}("));
    let mut items = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            items.extend([d.text(","), Doc::SOFT_LINE_OR_SPACE]);
        }
        items.push(d.text(arg));
    }
    let args = d.concat(items);
    let trailing = d.if_break(d.text(","), Doc::NIL);
    let indented = d.indent(d.concat([Doc::SOFT_LINE, args, trailing]));
    d.group(d.concat([open, indented, Doc::SOFT_LINE, d.text(")")]))
}

#[test]
fn text_and_concat_print_as_written() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([d.text("hello"), Doc::SPACE, d.text("world")]);
    assert_eq!(print(root, 80), "hello world");
}

#[test]
fn group_stays_flat_when_it_fits() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = call(d, "f", &["a", "b", "c"]);
    assert_eq!(print(root, 80), "f(a, b, c)");
}

#[test]
fn group_breaks_every_soft_line_when_it_does_not_fit() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = call(d, "f", &["aaaa", "bbbb", "cccc"]);
    assert_eq!(print(root, 12), "f(\n    aaaa,\n    bbbb,\n    cccc,\n)");
}

#[test]
fn nested_group_fits_flat_inside_a_broken_parent() {
    // Wadler's classic: the outer call breaks, the inner one stays flat.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let inner = call(d, "g", &["b", "c"]);
    let args = d.concat([
        Doc::SOFT_LINE,
        d.text("aaaaaaaa"),
        d.text(","),
        Doc::SOFT_LINE_OR_SPACE,
        inner,
    ]);
    let root = d.group(d.concat([d.text("f("), d.indent(args), Doc::SOFT_LINE, d.text(")")]));
    assert_eq!(print(root, 16), "f(\n    aaaaaaaa,\n    g(b, c)\n)");
}

#[test]
fn hard_line_forces_the_enclosing_group_to_break_but_not_its_siblings() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let stmt = d.text("x");
    let body = d.concat([Doc::HARD_LINE, stmt, Doc::HARD_LINE, stmt]);
    let block = d.group(d.concat([d.text("{"), d.indent(body), Doc::HARD_LINE, d.text("}")]));
    let sibling = call(d, "f", &["a", "b"]);
    let root = d.concat([sibling, Doc::SPACE, block]);
    assert_eq!(print(root, 80), "f(a, b) {\n    x\n    x\n}");
}

#[test]
fn expand_parent_breaks_the_group_it_is_in() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.group(d.concat([
        d.text("x"),
        Doc::SOFT_LINE_OR_SPACE,
        d.text("y"),
        Doc::EXPAND_PARENT,
    ]));
    assert_eq!(print(root, 80), "x\ny");
}

#[test]
fn if_group_breaks_follows_a_named_group_printed_earlier() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let body = d.concat([
        d.text("aaaaaaaaaa"),
        Doc::SOFT_LINE_OR_SPACE,
        d.text("bbbbbbbbbb"),
    ]);
    let (group, id) = d.group_with_id(body);
    let marker = d.if_group_breaks(id, d.text(" // broken"), d.text(" // flat"));
    let root = d.concat([group, marker]);
    assert_eq!(print(root, 80), "aaaaaaaaaa bbbbbbbbbb // flat");
    assert_eq!(print(root, 15), "aaaaaaaaaa\nbbbbbbbbbb // broken");
}

#[test]
fn line_suffix_prints_before_the_next_newline() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let suffix = d.line_suffix(" // c", SuffixWidth::Ignored);
    let root = d.concat([
        d.text("x"),
        suffix,
        d.text(";"),
        Doc::HARD_LINE,
        d.text("y"),
    ]);
    assert_eq!(print(root, 80), "x; // c\ny");
}

#[test]
fn line_suffix_boundary_flushes_a_pending_suffix() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let suffix = d.line_suffix(" // c", SuffixWidth::Ignored);
    let root = d.concat([d.text("x"), suffix, Doc::LINE_SUFFIX_BOUNDARY, d.text("y")]);
    assert_eq!(print(root, 80), "x // c\ny");
}

#[test]
fn a_trailing_line_suffix_is_printed_at_the_end() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let suffix = d.line_suffix(" // c", SuffixWidth::Ignored);
    let root = d.concat([d.text("x"), suffix]);
    assert_eq!(print(root, 80), "x // c");
}

#[test]
fn a_trailing_line_follows_the_line_it_was_emitted_on_at_its_indent() {
    // The comment is emitted while printing `b`, before the `,` and the
    // closing `]`, yet it lands after the `,` on a line of its own, indented
    // like `b`, with `]` after it.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let items = d.concat([
        Doc::HARD_LINE,
        d.text("a"),
        d.text(","),
        Doc::HARD_LINE,
        d.text("b"),
        d.trailing_line("// after b", false),
        d.text(","),
    ]);
    let root = d.concat([d.text("["), d.indent(items), Doc::HARD_LINE, d.text("]")]);
    assert_eq!(print(root, 80), "[\n    a,\n    b,\n    // after b\n]");
}

#[test]
fn a_trailing_line_can_keep_a_blank_line_before_it_and_follows_the_tails() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("x"),
        d.trailing_line("// own line", true),
        d.line_suffix(" // same line", SuffixWidth::Ignored),
        d.text(";"),
        Doc::HARD_LINE,
        d.text("y"),
    ]);
    assert_eq!(print(root, 80), "x; // same line\n\n// own line\ny");
}

#[test]
fn a_trailing_line_keeps_the_indent_it_was_emitted_at() {
    // After a multi-line text the line it ends on starts at column zero; the
    // comment still sits at the indent of the item it trails.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let body = d.concat([
        Doc::HARD_LINE,
        d.text("x = "),
        d.text("\"a\nb\""),
        d.text(";"),
        d.trailing_line("// c", false),
    ]);
    let root = d.concat([d.text("{"), d.indent(body), Doc::HARD_LINE, d.text("}")]);
    assert_eq!(print(root, 80), "{\n    x = \"a\nb\";\n    // c\n}");
}

#[test]
fn a_trailing_line_on_a_line_with_nothing_on_it_takes_that_line() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("a"),
        Doc::HARD_LINE,
        d.trailing_line("// t", false),
        Doc::HARD_LINE,
        d.text("b"),
    ]);
    assert_eq!(print(root, 80), "a\n// t\nb");
}

#[test]
fn a_trailing_line_forces_the_enclosing_group_to_break() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let args = d.concat([
        Doc::SOFT_LINE,
        d.text("a"),
        d.trailing_line("// c", false),
        d.if_break(d.text(","), Doc::NIL),
    ]);
    let root = d.group(d.concat([d.text("f("), d.indent(args), Doc::SOFT_LINE, d.text(")")]));
    assert_eq!(
        root.to_string(),
        "(group (concat \"f(\" (indent (concat softline \"a\" (trailing_line \"// c\") (if_break \",\" (concat)))) softline \")\"))"
    );
    assert_eq!(print(root, 80), "f(\n    a,\n    // c\n)");
}

#[test]
fn empty_line_leaves_exactly_one_blank_line() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let x = d.text("x");
    let y = d.text("y");
    let root = d.concat([
        x,
        Doc::EMPTY_LINE,
        Doc::EMPTY_LINE,
        y,
        Doc::HARD_LINE,
        Doc::EMPTY_LINE,
        x,
    ]);
    assert_eq!(print(root, 80), "x\n\ny\n\nx");
}

#[test]
fn indentation_is_lazy_so_blank_lines_carry_no_whitespace() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let body = d.concat([Doc::HARD_LINE, d.text("x"), Doc::EMPTY_LINE, d.text("y")]);
    let root = d.concat([d.text("{"), d.indent(body), Doc::HARD_LINE, d.text("}")]);
    assert_eq!(print(root, 80), "{\n    x\n\n    y\n}");
}

#[test]
fn a_space_before_a_newline_is_trimmed() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([d.text("x"), Doc::SPACE, Doc::HARD_LINE, d.text("y")]);
    assert_eq!(print(root, 80), "x\ny");
}

#[test]
fn multi_line_text_keeps_its_own_columns_and_breaks_the_enclosing_group() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.group(d.concat([
        d.text("f("),
        d.text("`a\n  b`"),
        d.text(")"),
        Doc::SOFT_LINE_OR_SPACE,
        d.text("tail"),
    ]));
    assert_eq!(print(root, 80), "f(`a\n  b`)\ntail");
}

#[test]
fn multi_line_text_inner_lines_keep_their_trailing_spaces() {
    // Trailing spaces inside a multi-line string are content; only lines
    // the printer ends itself are trimmed.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("x"),
        Doc::SPACE,
        d.text("\"a  \n  b\""),
        Doc::SPACE,
        Doc::HARD_LINE,
        d.text("y"),
    ]);
    assert_eq!(print(root, 80), "x \"a  \n  b\"\ny");
}

/// `f(a, b, (x) => { body })`: flat → hug the last argument → one per line.
fn hugging_call<'a>(d: Docs<'a>, args: &[&'a str], body_len: usize) -> Doc<'a> {
    let lambda = {
        let stmt = d.copy_text(&"y".repeat(body_len));
        d.expanded_group(d.concat([
            d.text("(x) => {"),
            d.indent(d.concat([Doc::HARD_LINE, stmt])),
            Doc::HARD_LINE,
            d.text("}"),
        ]))
    };
    let leading: Vec<Doc<'a>> = args
        .iter()
        .map(|arg| d.concat([d.text(arg), d.text(","), Doc::SOFT_LINE_OR_SPACE]))
        .collect();

    // flat: everything on one line. The lambda's hard lines make it a
    // forced layout, which `best_fitting` leaves out: a block body never
    // prints flat.
    let flat = d.concat([
        d.text("f("),
        d.concat(leading.iter().copied()),
        lambda,
        d.text(")"),
    ]);
    // hug: leading args flat, lambda expanded against the paren.
    let hug = d.concat([
        d.text("f("),
        d.group(d.concat(leading.iter().copied())),
        lambda,
        d.text(")"),
    ]);
    // expanded: one argument per line.
    let expanded = {
        let mut items = vec![Doc::HARD_LINE];
        for arg in args {
            items.extend([d.text(arg), d.text(","), Doc::HARD_LINE]);
        }
        items.extend([lambda, d.text(",")]);
        d.concat([
            d.text("f("),
            d.indent(d.concat(items)),
            Doc::HARD_LINE,
            d.text(")"),
        ])
    };
    d.best_fitting([flat, hug, expanded])
}

#[test]
fn best_fitting_hugs_when_the_leading_arguments_fit() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = hugging_call(d, &["a", "b"], 3);
    assert_eq!(print(root, 80), "f(a, b, (x) => {\n    yyy\n})");
}

#[test]
fn best_fitting_falls_back_to_the_most_expanded_variant() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = hugging_call(d, &["aaaaaaaa", "bbbbbbbb"], 3);
    assert_eq!(
        print(root, 20),
        "f(\n    aaaaaaaa,\n    bbbbbbbb,\n    (x) => {\n        yyy\n    },\n)"
    );
}

#[test]
fn best_fitting_measures_a_variant_to_its_first_line() {
    // The hug's first line fits in 20 columns and the lambda body line does
    // not; the hug is still chosen, as in Prettier and Biome: expanding the
    // call would only put the body deeper.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = hugging_call(d, &["a"], 30);
    assert_eq!(
        print(root, 20),
        "f(a, (x) => {\n    yyyyyyyyyyyyyyyyyyyyyyyyyyyyyy\n})"
    );
}

/// `let x = rhs;` with the right-hand side on the same line when it fits
/// there flat, else on a line of its own when it fits there whole, else on
/// the same line broken: rustfmt's `choose_rhs`.
fn assignment<'a>(d: Docs<'a>, args: &[&'a str]) -> Doc<'a> {
    let rhs = call(d, "f", args);
    d.concat([
        d.text("let x ="),
        d.best_fitting([
            d.concat([Doc::SPACE, rhs]).into(),
            d.indent(d.concat([Doc::HARD_LINE, rhs])).all_lines(),
            d.concat([Doc::SPACE, rhs]).into(),
        ]),
        d.text(";"),
    ])
}

#[test]
fn a_variant_measured_on_all_lines_is_chosen_when_every_line_fits() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = assignment(d, &["aaaa", "bbbb"]);
    assert_eq!(print(root, 80), "let x = f(aaaa, bbbb);");
    // Not on the `=` line; whole on the next one.
    assert_eq!(print(root, 20), "let x =\n    f(aaaa, bbbb);");
    // Measured by its first line alone, the next-line variant would always
    // be chosen: its first line is empty. Measured whole, it is not, and the
    // right-hand side breaks on the `=` line instead.
    assert_eq!(print(root, 16), "let x = f(\n    aaaa,\n    bbbb,\n);");
}

#[test]
fn a_variant_measured_on_all_lines_counts_what_follows_its_last_line() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // `f(aaaa, bbbb)` is 13 columns, at indent 4 that is 17, and the `;`
    // after the variant makes 18: it fits in 18 and not in 17.
    let root = assignment(d, &["aaaa", "bbbb"]);
    assert_eq!(print(root, 18), "let x =\n    f(aaaa, bbbb);");
    assert_eq!(print(root, 17), "let x = f(\n    aaaa,\n    bbbb,\n);");
}

#[test]
fn a_variant_measured_on_all_lines_prints_flat_with_its_own_breaks() {
    // A member chain broken one call per line, chosen only when each line
    // fits: the groups inside print flat without being measured again
    // before the first break, and are measured again after it.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let first = call(d, ".first", &["a"]);
    let last = call(d, ".last", &["bbbb", "cccc"]);
    let flat = d.concat([d.text("x"), first, last]);
    let expanded = d.concat([
        d.text("x"),
        d.indent(d.concat([Doc::HARD_LINE, first, Doc::HARD_LINE, last])),
    ]);
    let tail_broken = d.concat([d.text("x"), first, d.expanded_group(last)]);
    let root = d.best_fitting([
        flat.into(),
        expanded.all_lines(),
        tail_broken.into(),
        expanded.into(),
    ]);
    assert_eq!(print(root, 80), "x.first(a).last(bbbb, cccc)");
    assert_eq!(print(root, 22), "x\n    .first(a)\n    .last(bbbb, cccc)");
    // Neither line of the broken chain fits whole; the arguments break on
    // the flat chain's line instead.
    assert_eq!(print(root, 18), "x.first(a).last(\n    bbbb,\n    cccc,\n)");
    let shape = d.best_fitting([
        d.text("a").into(),
        d.text("b").all_lines(),
        d.text("c").into(),
    ]);
    assert_eq!(
        shape.to_string(),
        "(best_fitting \"a\" (all_lines \"b\") \"c\")"
    );
}

#[test]
fn a_space_at_the_start_of_a_line_prints_nothing() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("a"),
        Doc::HARD_LINE,
        Doc::SPACE,
        d.text("b"),
        d.group(d.concat([Doc::HARD_LINE, Doc::SOFT_LINE_OR_SPACE, d.text("c")])),
    ]);
    assert_eq!(print(root, 80), "a\nb\nc");
}

#[test]
fn an_if_break_forces_its_group_only_through_the_flat_arm() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // `{` on the same line, or on a line of its own once the signature broke:
    // the hard line in the breaks arm does not force the signature's group,
    // nor the enclosing one.
    let params = d.concat([
        d.text("f("),
        d.indent(d.concat([
            Doc::SOFT_LINE,
            d.text("a,"),
            Doc::SOFT_LINE_OR_SPACE,
            d.text("b"),
            d.if_break(d.text(","), Doc::NIL),
        ])),
        Doc::SOFT_LINE,
        d.text(")"),
    ]);
    let (signature, id) = d.group_with_id(params);
    let body = d.concat([
        signature,
        d.if_group_breaks(id, Doc::HARD_LINE, Doc::SPACE),
        d.text("{}"),
    ]);
    assert!(!body.forced());
    assert!(!body.will_break());
    assert_eq!(print(d.group(body), 80), "f(a, b) {}");
    assert_eq!(print(d.group(body), 8), "f(\n    a,\n    b,\n)\n{}");
    assert!(!d.if_break(Doc::HARD_LINE, Doc::NIL).forced());
    assert!(d.if_break(Doc::NIL, Doc::HARD_LINE).forced());
}

#[test]
fn a_forced_most_flat_variant_is_left_out_and_the_choice_breaks_its_parent() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let forced = d.concat([d.text("a"), Doc::HARD_LINE, d.text("b")]);
    // The one variant left is the fallback: it prints broken wherever the
    // choice sits, as a group it is forced to.
    let two = d.best_fitting([forced, d.text("z")]);
    assert_eq!(
        two.to_string(),
        "(concat expand_parent (group expanded \"z\"))"
    );
    let three = d.best_fitting([forced, d.text("y"), d.text("z")]);
    assert_eq!(
        three.to_string(),
        "(concat expand_parent (best_fitting \"y\" \"z\"))"
    );
    assert_eq!(print(three, 80), "y");
    // The content is sure to break whichever layout is chosen, so a group
    // around it breaks, as a group around a hard line does.
    let around = d.group(d.concat([
        d.text("["),
        Doc::SOFT_LINE,
        three,
        Doc::SOFT_LINE,
        d.text("]"),
    ]));
    assert!(around.will_break());
    assert_eq!(print(around, 80), "[\ny\n]");
}

#[test]
fn consecutive_hard_lines_leave_no_blank_line() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let a = d.text("a");
    let b = d.text("b");
    let hard_hard = d.concat([a, Doc::HARD_LINE, Doc::HARD_LINE, b]);
    assert_eq!(print(hard_hard, 80), "a\nb");
    let empty_hard = d.concat([a, Doc::EMPTY_LINE, Doc::HARD_LINE, b]);
    assert_eq!(print(empty_hard, 80), "a\n\nb");
    // A hard line on a blank line still sets the next line's indent.
    let indented = d.concat([a, Doc::HARD_LINE, d.indent(d.concat([Doc::HARD_LINE, b]))]);
    assert_eq!(print(indented, 80), "a\n    b");
}

#[test]
fn a_pending_suffix_ends_the_line_before_multi_line_text() {
    // Flushed after the text's last line, the comment would follow content
    // it was written before; flushed inside the text, it would be part of
    // it. So the line ends first, and the text starts the next.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("x = "),
        d.line_suffix(" // c", SuffixWidth::Ignored),
        d.text("\"a\nb\""),
        d.text(";"),
    ]);
    assert_eq!(print(root, 80), "x = // c\n\"a\nb\";");
    // A tail line pending ends it too.
    let tail_line = d.concat([
        d.text("x = "),
        d.trailing_line("// c", false),
        d.text("\"a\nb\""),
        d.text(";"),
    ]);
    assert_eq!(print(tail_line, 80), "x =\n// c\n\"a\nb\";");
}

#[test]
fn multi_line_text_is_written_exactly_at_its_edges() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    assert_eq!(print(d.text("\nabc"), 80), "\nabc");
    let ends_in_newline = d.concat([d.text("a\n"), Doc::HARD_LINE, d.text("x")]);
    assert_eq!(print(ends_in_newline, 80), "a\nx");
    let then_blank = d.concat([d.text("a\n"), Doc::EMPTY_LINE, d.text("x")]);
    assert_eq!(print(then_blank, 80), "a\n\nx");
}

#[test]
fn verbatim_lines_move_with_the_node() {
    // Written at indent 2 with its inner line at 6; printed at indent 4 the
    // inner line follows to 8 and the closing brace to 4. A blank line stays
    // blank, and text with no newline is plain text.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let block = d.verbatim("{\n      x\n\n  }", 2);
    let root = d.concat([d.text("f() "), d.indent(d.concat([Doc::HARD_LINE, block]))]);
    assert_eq!(print(root, 80), "f()\n    {\n        x\n\n    }");
    let shallower = d.concat([d.text("f() "), block]);
    assert_eq!(print(shallower, 80), "f() {\n    x\n\n}");
    assert_eq!(d.verbatim("x", 2).to_string(), "\"x\"");
}

#[test]
fn a_literal_is_an_atom_to_the_group_around_it() {
    // The group stays flat — it is measured by the literal's first line —
    // so the call hugs the template, whose lines then sit relative to the
    // line it starts on, not to the call's argument indent, and whose
    // closing line lands under that line.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let template = d.concat([d.text("`"), d.literal("\n      hello\n\n        there\n`")]);
    let call = d.group(d.concat([
        d.text("f("),
        d.indent(d.concat([Doc::SOFT_LINE, template])),
        Doc::SOFT_LINE,
        d.text(")"),
    ]));
    let root = d.indent(d.concat([Doc::HARD_LINE, call]));
    assert_eq!(
        print(root, 80),
        "    f(`\n          hello\n\n            there\n    `)"
    );
    assert!(!template.forced());
    assert!(template.spans_lines());
    assert!(!d.group(d.text("x")).spans_lines());
    assert_eq!(d.literal("x").to_string(), "\"x\"");
}

#[test]
fn a_space_before_a_suffix_is_trimmed_and_the_suffix_brings_its_own() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("a"),
        Doc::SPACE,
        d.line_suffix(" // c", SuffixWidth::Ignored),
        Doc::HARD_LINE,
        d.text("b"),
    ]);
    assert_eq!(print(root, 80), "a // c\nb");
}

#[test]
fn widths_count_an_emoji_sequence_once() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // A family emoji joined with ZWJ is one two-column glyph, not four.
    let root = call(d, "f", &["\u{1F468}\u{200D}\u{1F469}"]);
    assert_eq!(print(root, 5), "f(\u{1F468}\u{200D}\u{1F469})");
    assert_eq!(print(root, 4), "f(\n    \u{1F468}\u{200D}\u{1F469},\n)");
}

#[test]
fn a_nested_best_fitting_takes_its_parent_mode() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let inner = d.best_fitting([d.text("inner-flat"), d.text("inner-expanded")]);
    let outer_flat = d.concat([d.text("flat "), inner]);
    let outer_expanded = d.concat([d.text("expanded "), inner]);
    let root = d.best_fitting([outer_flat, outer_expanded]);
    assert_eq!(print(root, 80), "flat inner-flat");
    assert_eq!(print(root, 5), "expanded inner-expanded");
}

#[test]
fn widths_are_display_columns_not_bytes() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // "日本" is 6 bytes but 4 columns, so `=(日本)` is 7 columns, not 9.
    let root = call(d, "=", &["日本"]);
    assert_eq!(print(root, 7), "=(日本)");
    assert_eq!(print(root, 6), "=(\n    日本,\n)");
}

#[test]
fn text_borrows_the_source_and_measures_in_columns() {
    let source = "let 日本 = f(a)";
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // `日本` is bytes 4..10 of the source: 6 bytes, 4 columns.
    let name = d.text(&source[4..10]);
    let root = d.concat([name, d.text(" = "), call(d, "f", &["a"])]);
    assert_eq!(print(root, 80), "日本 = f(a)");
    assert_eq!(name.to_string(), "\"日本\"");
}

#[test]
fn display_renders_the_document_shape() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = call(d, "f", &["a"]);
    assert_eq!(
        root.to_string(),
        r#"(group (concat "f(" (indent (concat softline (concat "a") (if_break "," (concat)))) softline ")"))"#
    );
}

#[test]
fn display_numbers_only_the_groups_something_refers_to() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let (named, id) = d.group_with_id(d.text("a"));
    let (_unreferenced, _) = d.group_with_id(d.text("b"));
    let plain = d.group(d.text("c"));
    let marker = d.if_group_breaks(id, d.text("!"), Doc::NIL);
    let root = d.concat([named, plain, marker]);
    assert_eq!(
        root.to_string(),
        r#"(concat (group #1 "a") (group "c") (if_break #1 "!" (concat)))"#
    );
}

#[test]
fn no_line_exceeds_the_width_unless_an_atom_does() {
    // A property over nested calls at every width: each output line fits, or
    // is a single unbreakable piece that could not.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let mut root = d.text("leaf");
    for depth in 0..6 {
        let args = d.concat([
            Doc::SOFT_LINE,
            d.text("argument"),
            d.text(","),
            Doc::SOFT_LINE_OR_SPACE,
            root,
        ]);
        root = d.group(d.concat([
            d.copy_text(&format!("call{depth}(")),
            d.indent(args),
            Doc::SOFT_LINE,
            d.text(")"),
        ]));
    }
    for width in 8..60 {
        let out = print(root, width);
        for line in out.lines() {
            // One unbreakable piece plus its punctuation may exceed the width.
            let atom = line.trim().trim_end_matches([',', '(', ')']);
            assert!(
                line.chars().count() <= width || !atom.contains(' '),
                "width {width}: {line:?}"
            );
            assert!(
                !line.ends_with(' '),
                "trailing whitespace at width {width}: {line:?}"
            );
        }
        assert_eq!(print(root, width), out, "deterministic at width {width}");
    }
}

#[test]
fn a_line_suffix_reserving_its_width_can_break_the_line_before_it() {
    // `f(aaa, bbb)` is 11 columns and fits in 20 alone; with the 11-column
    // trailing comment counted, the group breaks.
    let build = |fit: SuffixWidth| {
        let bump = Bump::new();
        let d = Docs::new(&bump);
        let call = call(d, "f", &["aaa", "bbb"]);
        let suffix = d.line_suffix(" // comment", fit);
        print(d.concat([call, suffix]), 20)
    };
    assert_eq!(build(SuffixWidth::Ignored), "f(aaa, bbb) // comment");
    assert_eq!(
        build(SuffixWidth::Reserved),
        "f(\n    aaa,\n    bbb,\n) // comment"
    );
}

#[test]
fn newlines_at_the_start_of_the_output_print_nothing() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([Doc::EMPTY_LINE, Doc::HARD_LINE, d.text("x")]);
    assert_eq!(print(root, 80), "x");
}

#[test]
fn nil_prints_nothing_and_needs_no_arena() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([Doc::NIL, d.text("x"), Doc::NIL]);
    assert_eq!(print(root, 80), "x");
    assert_eq!(Doc::NIL.to_string(), "(concat)");
    assert_eq!(print(Doc::NIL, 80), "");
}

#[test]
fn a_group_after_a_hard_line_inside_flat_content_is_measured_again() {
    // The hug variant fits by its first line and prints flat, but the long
    // call inside the hugged body comes after a hard line: it is measured
    // for itself and breaks, instead of inheriting the flat decision.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let inner_call = d.group(d.concat([
        d.text("g("),
        d.indent(d.concat([Doc::SOFT_LINE, d.text("aaaaaaaaaaaaaaaaaaaa")])),
        Doc::SOFT_LINE,
        d.text(")"),
    ]));
    let body = d.concat([
        d.text("{"),
        d.indent(d.concat([Doc::HARD_LINE, inner_call])),
        Doc::HARD_LINE,
        d.text("}"),
    ]);
    let hug = d.concat([d.text("f(x, (y) -> "), body, d.text(")")]);
    let broken = d.concat([
        d.text("f("),
        d.indent(d.concat([
            Doc::HARD_LINE,
            d.text("x,"),
            Doc::HARD_LINE,
            d.text("(y) -> "),
            body,
        ])),
        Doc::HARD_LINE,
        d.text(")"),
    ]);
    let root = d.best_fitting([
        d.text("f(x, (y) -> { g(aaaaaaaaaaaaaaaaaaaa) })"),
        hug,
        broken,
    ]);
    assert_eq!(
        print(root, 24),
        "f(x, (y) -> {\n    g(\n        aaaaaaaaaaaaaaaaaaaa\n    )\n})"
    );
}

#[test]
fn a_layout_choice_after_a_hard_line_inside_flat_content_is_measured_again() {
    // As for a group: the hug variant prints flat, but a `let`'s choice of
    // layouts inside the hugged body comes after a hard line and is decided
    // for itself — the initialiser moves to a line of its own.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let init = d.text("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let assigned = d.best_fitting([
        d.concat([Doc::SPACE, init]).into(),
        d.indent(d.concat([Doc::HARD_LINE, init])).all_lines(),
        d.concat([Doc::SPACE, init]).into(),
    ]);
    let statement = d.concat([d.text("let x ="), assigned, d.text(";")]);
    let body = d.group(d.concat([
        d.text("{"),
        d.indent(d.concat([Doc::HARD_LINE, statement])),
        Doc::HARD_LINE,
        d.text("}"),
    ]));
    let hug = d.concat([d.text("f(x, (y) -> "), body, d.text(")")]);
    let broken = d.concat([
        d.text("f("),
        d.indent(d.concat([
            Doc::HARD_LINE,
            d.text("x,"),
            Doc::HARD_LINE,
            d.text("(y) -> "),
            body,
            d.text(","),
        ])),
        Doc::HARD_LINE,
        d.text(")"),
    ]);
    let root = d.best_fitting([
        d.text("f(x, (y) -> { let x = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; })"),
        hug,
        broken,
    ]);
    assert_eq!(
        print(root, 40),
        "f(x, (y) -> {\n    let x =\n        aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa;\n})"
    );
}

#[test]
fn variants_but_the_last_are_measured_as_they_print_flat() {
    // The content before a choice is measured with the choice after it. Its
    // first variant would print flat, so it is measured flat: `(eeeeee` does
    // not fit after `ab cd`, and the group before the choice breaks for it.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let before = d.group(d.concat([d.text("ab"), Doc::SOFT_LINE_OR_SPACE, d.text("cd")]));
    let choice = d.best_fitting([
        d.concat([
            d.text("("),
            d.group(d.concat([Doc::SOFT_LINE, d.text("eeeeee")])),
        ]),
        d.text("ffffffffffffffffffff"),
    ]);
    assert_eq!(print(d.concat([before, choice]), 10), "ab\ncd(eeeeee");
}

#[test]
fn measuring_all_lines_goes_on_past_a_text_with_lines_of_its_own() {
    // The variant on a line of its own is chosen only when every line of it
    // fits: what follows a multi-line string on its last line counts.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let init = d.concat([
        d.copy_text("\"a\nb\""),
        d.text(" + zzzzzzzzzzzzzzzzzzzzzzzz"),
    ]);
    let assigned = d.best_fitting([
        d.concat([Doc::SPACE, init]).into(),
        d.indent(d.concat([Doc::HARD_LINE, init])).all_lines(),
        d.concat([Doc::SPACE, init]).into(),
    ]);
    let root = d.concat([d.text("let x ="), assigned, d.text(";")]);
    assert_eq!(
        print(root, 20),
        "let x = \"a\nb\" + zzzzzzzzzzzzzzzzzzzzzzzz;"
    );
}

#[test]
fn reindented_lines_keep_their_own_whitespace() {
    // A literal's lines move with the line it starts on by gaining the
    // indent; what they had of their own — a tab, two spaces — stays, so the
    // string's value is what it was. Source text with a source indent loses
    // exactly that many columns of its leading whitespace, and no more.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let literal = d.literal("`\n  a\n\tb\n\n`");
    let root = d.indent(d.concat([Doc::HARD_LINE, d.text("x = "), literal]));
    assert_eq!(print(root, 80), "    x = `\n      a\n    \tb\n\n    `");
    let code = d.verbatim("f {\n\t\tx\n\t}", 1);
    let root = d.indent(d.concat([Doc::HARD_LINE, code]));
    assert_eq!(print(root, 80), "    f {\n    \tx\n    }");
}

#[test]
fn nested_layout_choices_are_measured_in_polynomial_time() {
    // A sole-argument hug nests a choice in a choice at every level; each is
    // measured once per room, not once per variant of every choice around
    // it. Deep nesting that used to take minutes prints at once.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let mut inner =
        d.text("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
    for level in 0..24 {
        let name = d.copy_text(&format!("f{level}("));
        let flat = d.concat([name, inner, d.text(")")]);
        let hug = d.concat([name, d.group(inner), d.text(")")]);
        let expanded = d.concat([
            name,
            d.indent(d.concat([Doc::SOFT_LINE, inner, d.if_break(d.text(","), Doc::NIL)])),
            Doc::SOFT_LINE,
            d.text(")"),
        ]);
        inner = d.best_fitting([
            flat.into(),
            hug.into(),
            d.group(expanded).all_lines(),
            d.group(expanded).into(),
        ]);
    }
    let output = print(inner, 100);
    assert!(output.lines().count() > 1);
}

#[test]
fn a_closing_suffix_ends_the_line_when_no_suffix_is_pending() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([d.text("x"), d.closing_suffix("// c"), d.text(";")]);
    assert_eq!(print(root, 80), "x; // c");
}

#[test]
fn a_closing_suffix_takes_a_line_of_its_own_after_a_pending_suffix() {
    // Two line comments on one line would read as one: the carried comment
    // gives way to the one written at the end of the line.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.concat([
        d.text("x"),
        d.line_suffix(" // t", SuffixWidth::Ignored),
        d.closing_suffix("// c"),
        d.text(";"),
    ]);
    assert_eq!(print(root, 80), "x; // t\n// c");
}

#[test]
fn a_closing_line_takes_the_level_of_the_item_it_was_carried_out_of() {
    // Emitted at the continuation indent of `-a`, the comment lands at the
    // statement's level, where a comment between statements reads as one of
    // theirs; the item's level holds past a dedent (the last item before
    // `}`), and gives way to a deeper line break that follows (the body
    // after a signature).
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let first = d.scope(d.concat([
        d.text("let x ="),
        d.indent(d.concat([
            Doc::HARD_LINE,
            d.text("-a"),
            d.closing_line("// c"),
            d.text(";"),
        ])),
    ]));
    let last = d.scope(d.concat([d.text("-b"), d.closing_line("// d")]));
    let block = d.concat([
        d.text("{"),
        d.indent(d.concat([Doc::HARD_LINE, first, Doc::HARD_LINE, last])),
        Doc::HARD_LINE,
        d.text("}"),
    ]);
    assert_eq!(
        print(block, 80),
        "{\n    let x =\n        -a;\n    // c\n    -b\n    // d\n}"
    );
    let signature = d.scope(d.concat([d.text("function f()"), d.closing_line("// e")]));
    let function = d.concat([
        signature,
        d.text(" {"),
        d.indent(d.concat([Doc::HARD_LINE, d.text("body")])),
        Doc::HARD_LINE,
        d.text("}"),
    ]);
    assert_eq!(print(function, 80), "function f() {\n    // e\n    body\n}");
}

#[test]
fn a_verbatim_text_is_measured_from_the_indent_of_its_line() {
    // The literal's second line prints at the indent of the line the literal
    // starts on, not at the indent of the document around it, and is
    // measured the same way: `cd)` is three columns, not seven.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let whole = d.concat([d.text("("), d.literal("ab\ncd"), d.text(")")]);
    let root = d.concat([
        d.text("f("),
        d.indent(d.best_fitting([whole.all_lines(), d.text("X").into()])),
    ]);
    assert_eq!(print(root, 5), "f((ab\ncd)");
}

#[test]
fn a_limit_breaks_a_group_wider_than_it_though_the_line_has_room() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let args = |items: &[&'static str]| {
        let mut parts = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                parts.extend([d.text(","), Doc::SOFT_LINE_OR_SPACE]);
            }
            parts.push(d.text(item));
        }
        d.concat(parts)
    };
    let call = |max_width: u32, items: &[&'static str]| {
        let inside = d.concat([
            Doc::SOFT_LINE,
            args(items),
            d.if_break(d.text(","), Doc::NIL),
        ]);
        d.group(d.concat([
            d.text("f("),
            d.indent(d.limited(inside, max_width)),
            Doc::SOFT_LINE,
            d.text(")"),
        ]))
    };
    // `aaa, bbb` is eight columns: within a limit of eight, past one of
    // seven, whatever the line width.
    assert_eq!(print(call(8, &["aaa", "bbb"]), 80), "f(aaa, bbb)");
    assert_eq!(
        print(call(7, &["aaa", "bbb"]), 80),
        "f(\n    aaa,\n    bbb,\n)"
    );
}

#[test]
fn a_limit_gives_its_columns_back_to_what_follows_its_content() {
    // Twenty columns, three of them for `abc` under a limit of three: the
    // seventeen withheld come back for ` tail`, which fits.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.group(d.concat([
        d.limited(d.text("abc"), 3),
        Doc::SOFT_LINE_OR_SPACE,
        d.text("tail"),
    ]));
    assert_eq!(print(root, 20), "abc tail");
    assert_eq!(print(root, 7), "abc\ntail");
}

#[test]
fn a_limit_measures_only_the_first_line_of_its_content() {
    // The variant's first line is `ab`, within a limit of two; the long
    // line after the hard break is not the limit's business. (`layouts`,
    // since `best_fitting` would drop a most flat variant that is sure to
    // break.)
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let hugged = |max_width| {
        let content = d.concat([d.text("ab"), Doc::HARD_LINE, d.text("cccccccccc")]);
        d.layouts([d.limited(content, max_width), d.text("X")])
    };
    assert_eq!(print(hugged(2), 80), "ab\ncccccccccc");
    assert_eq!(print(hugged(1), 80), "X");
}

#[test]
fn a_limit_is_transparent_in_a_broken_group_and_counts_a_space_inside_it() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    // Broken anyway, the limit changes nothing about how the content prints.
    let broken = d.expanded_group(d.concat([
        d.text("("),
        d.indent(d.concat([Doc::SOFT_LINE, d.limited(d.text("aaaa"), 2)])),
        Doc::SOFT_LINE,
        d.text(")"),
    ]));
    assert_eq!(print(broken, 80), "(\n    aaaa\n)");
    // A space inside the limit counts against it like any column: `{ ab }`
    // keeps its padding outside the limit, so `ab` is within two.
    let padded = |max_width| {
        d.group(d.concat([
            d.text("{"),
            d.indent(d.concat([Doc::SOFT_LINE_OR_SPACE, d.limited(d.text("ab"), max_width)])),
            Doc::SOFT_LINE_OR_SPACE,
            d.text("}"),
        ]))
    };
    assert_eq!(print(padded(2), 80), "{ ab }");
    assert_eq!(print(padded(1), 80), "{\n    ab\n}");
    let padded_inside = d.group(d.concat([
        d.text("{"),
        d.indent(d.limited(d.concat([Doc::SOFT_LINE_OR_SPACE, d.text("ab")]), 2)),
        Doc::SOFT_LINE_OR_SPACE,
        d.text("}"),
    ]));
    assert_eq!(print(padded_inside, 80), "{\n    ab\n}");
}

#[test]
fn a_snuggle_joins_a_line_of_closing_brackets_when_its_content_fits() {
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let closing = |operand: &'static str| {
        d.expanded_group(d.concat([
            d.text("f("),
            d.indent(d.concat([Doc::HARD_LINE, d.text("a")])),
            Doc::HARD_LINE,
            d.text(")"),
            d.indent(d.snuggle(d.concat([d.text("+ "), d.text(operand)]))),
        ]))
    };
    // `)` ends at column one, within the four the newline would indent to,
    // and `+ x` fits after it.
    assert_eq!(print(closing("x"), 80), "f(\n    a\n) + x");
    // It would not fit: the newline it is.
    assert_eq!(print(closing("xxxxx"), 6), "f(\n    a\n)\n    + xxxxx");
    // A line that reaches past the indent column keeps the newline.
    let long = d.expanded_group(d.concat([d.text("aaaaaa"), d.indent(d.snuggle(d.text("+ x")))]));
    assert_eq!(print(long, 80), "aaaaaa\n    + x");
    // Flat, it is a space like any soft line.
    let flat = d.group(d.concat([d.text("a"), d.indent(d.snuggle(d.text("+ x")))]));
    assert_eq!(print(flat, 80), "a + x");
}

#[test]
fn a_snuggle_keeps_the_newline_after_a_pending_suffix() {
    // The `)` line ends with a comment: `+ x` cannot follow it there.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.expanded_group(d.concat([
        d.text("f("),
        d.indent(d.concat([Doc::HARD_LINE, d.text("a")])),
        Doc::HARD_LINE,
        d.text(")"),
        d.line_suffix(" // c", SuffixWidth::Ignored),
        d.indent(d.snuggle(d.text("+ x"))),
    ]));
    assert_eq!(print(root, 80), "f(\n    a\n) // c\n    + x");
}

#[test]
fn a_snuggle_counts_the_space_it_would_print() {
    // `) abc` is five columns: at a width of four the space does not fit,
    // so the newline it is, though `abc` alone would have fit after `)`.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let root = d.expanded_group(d.concat([
        d.text("f("),
        d.indent(d.concat([Doc::HARD_LINE, d.text("a")])),
        Doc::HARD_LINE,
        d.text(")"),
        d.indent(d.snuggle(d.text("abc"))),
    ]));
    assert_eq!(print(root, 5), "f(\n    a\n) abc");
    assert_eq!(print(root, 4), "f(\n    a\n)\n    abc");
}

#[test]
fn a_conditional_on_a_group_sure_to_break_is_measured_as_broken_before_the_group_prints() {
    // The named group holds a hard line, so it prints broken wherever it
    // is; a variant measured before it printed must count on that, or it
    // would be chosen for a `NIL` arm that never prints.
    let bump = Bump::new();
    let d = Docs::new(&bump);
    let (group, id) = d.group_with_id(d.concat([d.text("a"), Doc::HARD_LINE, d.text("b")]));
    let candidate = d.concat([group, d.if_group_breaks(id, d.text("long"), Doc::NIL)]);
    let root = d.layouts([candidate.all_lines(), d.text("X").into()]);
    assert_eq!(print(root, 4), "X");
    assert_eq!(print(root, 5), "a\nblong");
}
