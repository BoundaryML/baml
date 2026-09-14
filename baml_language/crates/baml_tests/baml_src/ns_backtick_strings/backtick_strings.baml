// BEP-049 backtick string literals — end-to-end runtime tests.
//
// Converted from crates/bex_engine/tests/backtick_strings.rs. Each Rust test's
// `function main()` became a uniquely named top-level helper (they all share
// one namespace), and its `expected` became an `assert.equal` on the helper's
// call result. Test names are the Rust fn names verbatim.
//
// Two conventions the corpus forces on this file:
//
//   * Every assertion calls a helper. A `let`-bound local inside a `test` block
//     compares unequal to a literal (VM boxing bug), so nothing is staged in a
//     local here.
//   * Indentation inside a backtick literal is load-bearing: §12 dedent strips
//     the longest common leading-whitespace prefix of the content lines. Lines
//     that sit at column 0 in the original Rust fixture stay at column 0 here,
//     even where that looks wrong next to the enclosing function body.
//
// Compile-error cases (cases A, B, E — null / optional / strict interp typing)
// are not runtime tests; they live in
// projects/diagnostic_errors/backtick_strict_types/.

// ─── M1: text, dedent, escapes ───────────────────────────────────────────────

function bt_one_liner() -> string {
    `hello world`
}

test "backtick_one_liner_evaluates_to_string" {
    assert.equal(bt_one_liner(), "hello world")
}

// §12: multi-line content auto-dedents. The content lines' shared indent is
// stripped, and so are the line break after the opener and the one (plus
// indent) before the closer — those belong to the delimiters, not the content.
function bt_multiline_dedent() -> string {
    `
        line one
        line two
    `
}

test "backtick_multiline_dedents_at_runtime" {
    assert.equal(bt_multiline_dedent(), "line one\nline two")
}

// `\n` becomes a real newline; `` \` `` becomes a literal backtick; `\${`
// becomes a literal `${`.
function bt_escapes_decoded() -> string {
    `a\nb\`c\${d}e`
}

test "backtick_escapes_decoded_at_runtime" {
    assert.equal(bt_escapes_decoded(), "a\nb`c${d}e")
}

// ─── M2: ${...} interpolation ────────────────────────────────────────────────

function bt_simple_interp() -> string {
    let name = "world"
    `Hello, ${name}!`
}

test "backtick_simple_interpolation_evaluates_at_runtime" {
    assert.equal(bt_simple_interp(), "Hello, world!")
}

function bt_multi_interp() -> string {
    let first = "ada"
    let last = "lovelace"
    `${first} ${last} writes code`
}

test "backtick_multiple_interpolations_evaluate_at_runtime" {
    assert.equal(bt_multi_interp(), "ada lovelace writes code")
}

// BEP §4: `${...}` is a block expression — statements plus an optional trailing
// expression. The block's tail value is what gets interpolated.
function bt_block_body_tail() -> string {
    `result: ${ let prefix = "hi "; let name = "there"; prefix + name }!`
}

test "backtick_block_body_with_let_renders_tail_expression" {
    assert.equal(bt_block_body_tail(), "result: hi there!")
}

function bt_icf_shout(s: string) -> string {
    s + "!!!"
}

function bt_icf_main() -> string {
    `Loud: ${bt_icf_shout("hi")} and ${bt_icf_shout("bye")}`
}

test "backtick_interpolation_calls_function" {
    assert.equal(bt_icf_main(), "Loud: hi!!! and bye!!!")
}

// Cursed but legal: a while loop builds a value inside `${...}` and the final
// expression is what gets interpolated.
function bt_loop_in_block() -> string {
    `loop result: ${
        let acc = ""
        let i = 0
        while (i < 3) {
            acc = acc + "x"
            i = i + 1
        }
        acc
    }!`
}

test "backtick_interpolation_with_loop_in_block_body" {
    assert.equal(bt_loop_in_block(), "loop result: xxx!")
}

// BEP §4 worked example: bind an if-expression's value with a let, then
// interpolate it.
function bt_let_bound_if_else() -> string {
    let logged_in = true
    `welcome ${ let r = if (logged_in) { "user" } else { "guest" }; r }`
}

test "backtick_interpolation_with_let_bound_if_else_expression" {
    assert.equal(bt_let_bound_if_else(), "welcome user")
}

// `${if (cond) { "pos" } else { "neg" }}` is a valid inline if-EXPRESSION
// render. A runtime condition keeps the branches as a literal-union, and
// interpolating it resolves `.to_string()` through the union.
function bt_inline_if_classify(x: int) -> string {
    `${if (x > 0) { "pos" } else { "neg" }}`
}

function bt_inline_if_main() -> string {
    bt_inline_if_classify(5) + "/" + bt_inline_if_classify(-1)
}

test "backtick_inline_if_expression_renders" {
    assert.equal(bt_inline_if_main(), "pos/neg")
}

// Same for an int literal-union (`1 | 2`): the runtime value is an int, so
// `${…}` dispatches to `Int.to_string`.
function bt_inline_if_int_pick(x: int) -> string {
    `${if (x > 0) { 1 } else { 2 }}`
}

function bt_inline_if_int_main() -> string {
    bt_inline_if_int_pick(5) + "/" + bt_inline_if_int_pick(-1)
}

test "backtick_inline_if_int_union_renders" {
    assert.equal(bt_inline_if_int_main(), "1/2")
}

// BEP §4: a side-effect-only `${ let … }` defines a binding for the rest of the
// template (the `{% set %}` equivalent) — the untagged template lowers into one
// shared concat scope.
function bt_xslet_f(items: int[]) -> string {
    `count: ${ let n = items.length() }${n} items`
}

function bt_xslet_main() -> string {
    bt_xslet_f([1, 2, 3])
}

test "backtick_cross_site_let_leaks_to_later_segment" {
    assert.equal(bt_xslet_main(), "count: 3 items")
}

// BEP §4: a block whose tail evaluates to unit renders "". An `if` without
// `else` is void-typed; it renders "" rather than erroring on a missing
// `to_string`.
function bt_void_tail() -> string {
    `a${ if (false) { "x" } }b`
}

test "backtick_void_tail_renders_empty" {
    assert.equal(bt_void_tail(), "ab")
}

// A backtick string inside an interpolation inside another backtick string.
function bt_nested_interp() -> string {
    let inner = `inner-${"x"}`
    `outer[${inner}]`
}

test "backtick_nested_in_interpolation" {
    assert.equal(bt_nested_interp(), "outer[inner-x]")
}

function bt_bangs(n: int) -> string {
    if (n <= 0) { "" } else { "!" + bt_bangs(n - 1) }
}

function bt_recursive_main() -> string {
    `done${bt_bangs(5)}`
}

test "backtick_interpolation_with_recursive_call" {
    assert.equal(bt_recursive_main(), "done!!!!!")
}

// BEP §11: `${int}` works without an explicit `.to_string()` — the lowering
// wraps each interp with `.to_string()` and `Int` has a stdlib `to_string`.
function bt_interp_int() -> string {
    let count = 42
    `count: ${count}`
}

test "backtick_interpolation_int_auto_to_string" {
    assert.equal(bt_interp_int(), "count: 42")
}

function bt_interp_bool() -> string {
    let ok = true
    `status: ${ok}`
}

test "backtick_interpolation_bool_auto_to_string" {
    assert.equal(bt_interp_bool(), "status: true")
}

function bt_interp_float() -> string {
    let pi = 3.14
    `pi ~ ${pi}`
}

test "backtick_interpolation_float_auto_to_string" {
    assert.equal(bt_interp_float(), "pi ~ 3.14")
}

// Mix of int, bool, float, string in one backtick — every interp gets
// `.to_string()` at lowering, regardless of source type.
function bt_interp_mixed() -> string {
    let n = 7
    let ok = false
    let r = 1.5
    let name = "test"
    `n=${n}, ok=${ok}, r=${r}, name=${name}`
}

test "backtick_interpolation_mixed_types_auto_to_string" {
    assert.equal(bt_interp_mixed(), "n=7, ok=false, r=1.5, name=test")
}

// ─── M2 coverage cases (C–Q) ─────────────────────────────────────────────────

// (C) Empty interpolation `${}` — block with no stmts and no tail → unit, which
// per BEP §4 renders as "".
function bt_case_c() -> string {
    `a${}b`
}

test "backtick_case_c_empty_interp_renders_empty" {
    assert.equal(bt_case_c(), "ab")
}

// (D) Statement-only block `${ let x = 5 }` — block ends in a statement, no
// tail, so also unit → "".
function bt_case_d() -> string {
    `a${ let x = 5 }b`
}

test "backtick_case_d_statement_only_block_renders_empty" {
    assert.equal(bt_case_d(), "ab")
}

// The statement-only segment is spliced into ONE shared concat scope (like the
// `${for}` accumulator) rather than confined to its own block, so `w` resolves
// in the later segment.
function bt_xslet_concat_scope() -> string {
    `${ let w = "hi" }${w}`
}

test "backtick_cross_site_let_binds_into_concat_scope" {
    assert.equal(bt_xslet_concat_scope(), "hi")
}

// Later segments observe earlier segments' bindings in source order, and
// intervening text segments don't break visibility.
function bt_xslet_chain() -> string {
    `a${ let x = "X" }b${x}c${ let y = x + "Y" }d${y}e`
}

test "backtick_cross_site_let_chain_with_text" {
    assert.equal(bt_xslet_chain(), "abXcdXYe")
}

// Side-effect evaluation order survives the hoist: a local reassigned across
// segments reflects each mutation at the point the next value segment reads it.
function bt_xslet_reassign() -> string {
    let n = 0
    `${ n = n + 1 }${n}-${ n = n + 1 }${n}`
}

test "backtick_cross_site_let_reassign_evaluation_order" {
    assert.equal(bt_xslet_reassign(), "1-2")
}

// (F) Adjacent interpolations with no text between.
function bt_case_f() -> string {
    let a = "ab"
    let b = "cd"
    `${a}${b}`
}

test "backtick_case_f_adjacent_interpolations" {
    assert.equal(bt_case_f(), "abcd")
}

// (G) Interpolation at the very start (no leading text).
function bt_case_g() -> string {
    let x = "hi"
    `${x}!`
}

test "backtick_case_g_interp_at_start" {
    assert.equal(bt_case_g(), "hi!")
}

// (H) Interpolation at the very end (no trailing text).
function bt_case_h() -> string {
    let x = "hi"
    `!${x}`
}

test "backtick_case_h_interp_at_end" {
    assert.equal(bt_case_h(), "!hi")
}

// (I) Multi-tick delimiter plus interpolation.
function bt_case_i() -> string {
    let x = "Y"
    ``X ${x} Z``
}

test "backtick_case_i_multi_tick_with_interpolation" {
    assert.equal(bt_case_i(), "X Y Z")
}

// (J) A class controlling its own rendering via `implements baml.ToString` —
// `${p}` renders through `string.from`, which dispatches to the override.
class Person {
    name string
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self.name
        }
    }
}

function bt_case_j() -> string {
    let p = Person { name: "Ada" }
    `meet ${p}`
}

test "backtick_case_j_user_class_with_to_string" {
    assert.equal(bt_case_j(), "meet Ada")
}

// §11: a class that does NOT implement `baml.ToString` is still interpolatable
// — `string.from` falls back to a structural rendering of the instance.
class Point {
    x int
    y int
}

function bt_point_structural() -> string {
    let p = Point { x: 1, y: 2 }
    `p = ${p}`
}

test "backtick_interp_class_without_to_string_renders_structurally" {
    assert.equal(bt_point_structural(), "p = Point { x: 1, y: 2 }")
}

// (K) Backtick string as a parameter's default value.
function bt_case_k_greet(name: string, prefix: string = `Hello, `) -> string {
    prefix + name
}

function bt_case_k() -> string {
    bt_case_k_greet("world")
}

test "backtick_case_k_default_parameter_value" {
    assert.equal(bt_case_k(), "Hello, world")
}

// (L) Backtick used as an expression-statement (value discarded).
function bt_case_l() -> int {
    `unused`;
    42
}

test "backtick_case_l_expression_statement" {
    assert.equal(bt_case_l(), 42)
}

// (M) Two backtick strings concatenated with `+`.
function bt_case_m() -> string {
    `a` + `b`
}

test "backtick_case_m_two_backticks_concatenated" {
    assert.equal(bt_case_m(), "ab")
}

// (N) Backtick literal passed as a call argument.
function bt_case_n_shout(s: string) -> string { s + "!" }

function bt_case_n() -> string {
    let name = "Ada"
    bt_case_n_shout(`Hello ${name}`)
}

test "backtick_case_n_passed_as_function_argument" {
    assert.equal(bt_case_n(), "Hello Ada!")
}

// (O) Array indexing inside a `${...}` body.
function bt_case_o() -> string {
    let arr = ["a", "b", "c"]
    `first: ${arr[0]}`
}

test "backtick_case_o_array_index_in_interp" {
    assert.equal(bt_case_o(), "first: a")
}

// (P) Method chain inside `${...}` on a let-bound value.
function bt_case_p() -> string {
    let s = "hello"
    `upper: ${ let r = s.to_upper_case(); r }`
}

test "backtick_case_p_method_chain_on_let_bound" {
    assert.equal(bt_case_p(), "upper: HELLO")
}

// (Q) A throw inside `${...}` propagates out of the backtick. The Rust fixture
// asserted the uncaught `Err("boom")`; here the throw is caught at the test
// boundary and the arm type pins what escaped.
function bt_case_q() -> string {
    `x: ${ throw "boom" }`
}

test "backtick_case_q_throw_in_interp_body" {
    {
        let _ = bt_case_q();
        baml.sys.panic("expected the interpolated throw to propagate")
    } catch (e) {
        string => null
    }
}

// ─── TypeScript-parity cases (AA, BB, GG, HH, LL) ────────────────────────────


// (BB) Extended C-style escapes: \b, \v, \f. None of the three are BAML
// string-literal escapes, so the expected values are decoded from bytes.
function bt_escape_backspace() -> string {
    `a\bb`
}

test "backtick_extended_escape_backspace" {
    // \x08 = backspace
    assert.equal(bt_escape_backspace(), string.from_utf8(b"a\x08b"))
}

function bt_escape_vertical_tab() -> string {
    `a\vb`
}

test "backtick_extended_escape_vertical_tab" {
    // \x0B = vertical tab
    assert.equal(bt_escape_vertical_tab(), string.from_utf8(b"a\x0Bb"))
}

function bt_escape_form_feed() -> string {
    `a\fb`
}

test "backtick_extended_escape_form_feed" {
    // \x0C = form feed
    assert.equal(bt_escape_form_feed(), string.from_utf8(b"a\x0Cb"))
}

// (GG) Block comment inside a `${...}` body.
function bt_block_comment_interp() -> string {
    let name = "Ada"
    `hi ${ /* greet someone */ name }`
}

test "backtick_block_comment_inside_interp" {
    assert.equal(bt_block_comment_interp(), "hi Ada")
}

// (HH) Line comment inside a `${...}` body.
function bt_line_comment_interp() -> string {
    let name = "Ada"
    `hi ${
        // a friendly greeting
        name
    }`
}

test "backtick_line_comment_inside_interp" {
    assert.equal(bt_line_comment_interp(), "hi Ada")
}

// (LL) `$` at the very end of a template stays literal — no `{` follows.
function bt_trailing_dollar() -> string {
    `cost: $`
}

test "backtick_trailing_dollar_is_literal" {
    assert.equal(bt_trailing_dollar(), "cost: $")
}

// ─── M3: ${for} / ${if} block control flow ───────────────────────────────────

// BEP §5 block-tag form: each iteration's text plus interpolation is
// concatenated into the output.
function bt_m3_for_basic() -> string {
    let xs = ["a", "b", "c"]
    `${for (let x in xs)}- ${x}
${endfor}`
}

test "backtick_m3_for_loop_basic" {
    assert.equal(bt_m3_for_basic(), "- a\n- b\n- c\n")
}

// Text before and after the for-block flows in order.
function bt_m3_for_text_around() -> string {
    let xs = ["x", "y"]
    `header
${for (let v in xs)}${v}, ${endfor}done`
}

test "backtick_m3_for_loop_with_text_around" {
    assert.equal(bt_m3_for_text_around(), "header\nx, y, done")
}

function bt_m3_if_true() -> string {
    let logged_in = true
    `${if (logged_in)}welcome${endif}`
}

test "backtick_m3_if_block_tag_true_branch" {
    assert.equal(bt_m3_if_true(), "welcome")
}

function bt_m3_if_false() -> string {
    let logged_in = false
    `${if (logged_in)}welcome${endif}!`
}

test "backtick_m3_if_block_tag_false_branch_empty" {
    assert.equal(bt_m3_if_false(), "!")
}

function bt_m3_if_else_chain_classify(n: int) -> string {
    `${if (n > 0)}pos${else if (n < 0)}neg${else}zero${endif}`
}

function bt_m3_if_else_chain() -> string {
    bt_m3_if_else_chain_classify(5) + " " + bt_m3_if_else_chain_classify(-3) + " " + bt_m3_if_else_chain_classify(0)
}

test "backtick_m3_if_else_chain" {
    assert.equal(bt_m3_if_else_chain(), "pos neg zero")
}

function bt_m3_nested_for_in_if() -> string {
    let xs = ["a", "b"]
    let show = true
    `${if (show)}${for (let x in xs)}- ${x}
${endfor}${endif}`
}

test "backtick_m3_nested_for_in_if" {
    assert.equal(bt_m3_nested_for_in_if(), "- a\n- b\n")
}

function bt_m3_for_interp_body() -> string {
    let xs = [1, 2, 3]
    `${for (let x in xs)}${x},${endfor}`
}

test "backtick_m3_for_with_interp_in_body" {
    assert.equal(bt_m3_for_interp_body(), "1,2,3,")
}

// ─── M3 §13 whitespace-control worked cases ──────────────────────────────────

// Case 1: `${for}` and `${endfor}` alone on their lines, body line indented.
// Each iteration emits the indented body plus a trailing newline.
function bt_m3_ws_case1() -> string {
    let names = ["a", "b", "c"]
    `
${for (let n in names)}
  - ${n}
${endfor}
`
}

test "backtick_m3_ws_case1_items_on_own_lines" {
    assert.equal(bt_m3_ws_case1(), "  - a\n  - b\n  - c\n")
}

// Case 2: all tags inline — mid-line, so they consume nothing.
function bt_m3_ws_case2() -> string {
    let names = ["a", "b", "c"]
    `${for (let n in names)}${n}${endfor}`
}

test "backtick_m3_ws_case2_inline_concatenation" {
    assert.equal(bt_m3_ws_case2(), "abc")
}

// Case 3: a blank line inside the body becomes a separator.
function bt_m3_ws_case3() -> string {
    let names = ["a", "b", "c"]
    `
${for (let n in names)}
- ${n}

${endfor}
`
}

test "backtick_m3_ws_case3_blank_line_between_iterations" {
    assert.equal(bt_m3_ws_case3(), "- a\n\n- b\n\n- c\n\n")
}

// Case 6: full-line `${if}` / `${endif}` consume their own lines entirely, so
// the false branch leaves no extra newline behind. The line break before the
// closing backtick is the closer's own, so "Footer\n" lands as "Footer".
function bt_m3_ws_case6_classify(extra: bool) -> string {
    `
Header
${if (extra)}
Extra
${endif}
Footer
`
}

function bt_m3_ws_case6() -> string {
    bt_m3_ws_case6_classify(true) + "|" + bt_m3_ws_case6_classify(false)
}

test "backtick_m3_ws_case6_if_full_line_block" {
    assert.equal(bt_m3_ws_case6(), "Header\nExtra\nFooter|Header\nFooter")
}

// Case 7: `${if}` mid-line — the newline after it is body content, not
// block-tag whitespace, so it shows up only when the branch is taken.
function bt_m3_ws_case7_show(extra: bool) -> string {
    `
Header${if (extra)}
Extra line${endif}
Footer
`
}

function bt_m3_ws_case7() -> string {
    bt_m3_ws_case7_show(true) + "|" + bt_m3_ws_case7_show(false)
}

test "backtick_m3_ws_case7_if_introduces_content_only_when_true" {
    assert.equal(bt_m3_ws_case7(), "Header\nExtra line\nFooter|Header\nFooter")
}

// Case 8: nested `${for}` loops, each alone on its line. The trailing newline
// is the inner for-body's last `\n`, emitted by each outer iteration — body
// content, not block-tag whitespace.
class Group {
    name string
    items string[]
}

function bt_m3_ws_case8() -> string {
    let groups = [
        Group { name: "Fruits", items: ["apple", "banana"] },
        Group { name: "Drinks", items: ["coffee"] },
    ]
    `
${for (let g in groups)}
Group: ${g.name}
${for (let i in g.items)}
  - ${i}
${endfor}
${endfor}
`
}

test "backtick_m3_ws_case8_nested_loops_preserving_structure" {
    assert.equal(bt_m3_ws_case8(), "Group: Fruits\n  - apple\n  - banana\nGroup: Drinks\n  - coffee\n")
}

// ─── M3 edge cases ───────────────────────────────────────────────────────────

// Zero iterations → the accumulator stays empty; surrounding text still flows.
function bt_m3_for_empty_collection() -> string {
    let xs: string[] = []
    `before|${for (let x in xs)}- ${x}
${endfor}|after`
}

test "backtick_m3_for_loop_over_empty_collection" {
    assert.equal(bt_m3_for_empty_collection(), "before||after")
}

// A for-block with nothing between its tags contributes the empty string, even
// with N iterations.
function bt_m3_for_empty_body() -> string {
    let xs = [1, 2, 3]
    `[${for (let _x in xs)}${endfor}]`
}

test "backtick_m3_for_loop_empty_body" {
    assert.equal(bt_m3_for_empty_body(), "[]")
}

// The collection expression is an inline array literal.
function bt_m3_for_array_literal() -> string {
    `${for (let n in [10, 20, 30])}${n} ${endfor}`
}

test "backtick_m3_for_loop_over_array_literal" {
    assert.equal(bt_m3_for_array_literal(), "10 20 30 ")
}

// BEP §4: the C-style `for (init; cond; step)` header is accepted in `${for}`
// — the same headers as the host `for`, not just the iterator form.
function bt_cstyle_for_basic() -> string {
    `${for (let i = 0; i < 3; i += 1)}[${i}]${endfor}`
}

test "backtick_cstyle_for_basic" {
    assert.equal(bt_cstyle_for_basic(), "[0][1][2]")
}

// The C-style header reads an enclosing local in its condition.
function bt_cstyle_for_outer_local() -> string {
    let n = 4
    `${for (let i = 0; i < n; i += 1)}${i},${endfor}`
}

test "backtick_cstyle_for_bound_by_outer_local" {
    assert.equal(bt_cstyle_for_outer_local(), "0,1,2,3,")
}

function bt_cstyle_for_step_two() -> string {
    `${for (let i = 0; i < 6; i += 2)}${i} ${endfor}`
}

test "backtick_cstyle_for_step_by_two" {
    assert.equal(bt_cstyle_for_step_two(), "0 2 4 ")
}

// If-chain inside a for-body. The flag comes from an outer scope so the
// condition doesn't have to resolve against the for-binding's element type.
function bt_m3_nested_if_in_for_go(flag: bool, xs: string[]) -> string {
    `${for (let x in xs)}${if (flag)}+${x}${else}-${x}${endif} ${endfor}`
}

function bt_m3_nested_if_in_for() -> string {
    bt_m3_nested_if_in_for_go(true, ["a", "b"]) + "|" + bt_m3_nested_if_in_for_go(false, ["a", "b"])
}

test "backtick_m3_nested_if_in_for" {
    assert.equal(bt_m3_nested_if_in_for(), "+a +b |-a -b ")
}

// Case 4: a blank line between the for-block and a trailing Footer paragraph
// stays in the output.
function bt_m3_ws_case4() -> string {
    let names = ["a", "b", "c"]
    `
${for (let n in names)}
- ${n}
${endfor}

Footer
`
}

test "backtick_m3_ws_case4_blank_line_after_block" {
    assert.equal(bt_m3_ws_case4(), "- a\n- b\n- c\n\nFooter")
}

// Case 5: mid-line `${if}` consumes nothing.
function bt_m3_ws_case5_greet(formal: bool) -> string {
    `Hello${if (formal)}, sir${endif}!`
}

function bt_m3_ws_case5() -> string {
    bt_m3_ws_case5_greet(true) + "|" + bt_m3_ws_case5_greet(false)
}

test "backtick_m3_ws_case5_conditional_inline" {
    assert.equal(bt_m3_ws_case5(), "Hello, sir!|Hello!")
}

// §13 mid-line edge case: an indented `${for}` followed by body, `${endfor}`
// and a suffix on the same line is mid-line, so the leading 4 spaces stay in
// the output as literal text.
function bt_m3_ws_mid_line_indented_for() -> string {
    let names = ["a", "b", "c"]
    `
Prefix:
    ${for (let n in names)}${n} ${endfor}Suffix
`
}

test "backtick_m3_ws_mid_line_indented_for_keeps_indent_literal" {
    assert.equal(bt_m3_ws_mid_line_indented_for(), "Prefix:\n    a b c Suffix")
}

// No preceding text at all: the literal-start position counts as
// start-of-line, so `${for}` at offset 0 is alone and strips its trailing
// newline. `${endfor}` is followed by `done` on the same line, so it is
// mid-line and consumes nothing.
function bt_m3_ws_tag_at_literal_start() -> string {
    let xs = ["x", "y"]
    `${for (let n in xs)}
${n}
${endfor}done`
}

test "backtick_m3_ws_tag_at_literal_start" {
    assert.equal(bt_m3_ws_tag_at_literal_start(), "x\ny\ndone")
}

// The for-binding shadows an outer `let x` for the duration of the loop.
function bt_m3_for_shadows_outer_let() -> string {
    let x = "OUTER"
    let xs = ["a", "b"]
    let inner = `${for (let x in xs)}${x},${endfor}`
    inner + "|" + x
}

test "backtick_m3_for_loop_shadows_outer_let" {
    assert.equal(bt_m3_for_shadows_outer_let(), "a,b,|OUTER")
}

// `&&` / `||` / comparison in a block-tag condition.
function bt_m3_if_complex_cond_f(a: int, b: int) -> string {
    `${if (a > 0 && b > 0)}pos${else if (a < 0 || b < 0)}neg${else}zero${endif}`
}

function bt_m3_if_complex_cond() -> string {
    bt_m3_if_complex_cond_f(1, 1) + "|" + bt_m3_if_complex_cond_f(-1, 0) + "|" + bt_m3_if_complex_cond_f(0, 0)
}

test "backtick_m3_if_with_complex_condition" {
    assert.equal(bt_m3_if_complex_cond(), "pos|neg|zero")
}


// An if-block whose true branch contains a for-loop.
function bt_m3_for_inside_if_render(show: bool, xs: string[]) -> string {
    `${if (show)}${for (let x in xs)}<${x}>${endfor}${else}hidden${endif}`
}

function bt_m3_for_inside_if() -> string {
    bt_m3_for_inside_if_render(true, ["a", "b"]) + "|" + bt_m3_for_inside_if_render(false, ["a", "b"])
}

test "backtick_m3_for_inside_if_branch" {
    assert.equal(bt_m3_for_inside_if(), "<a><b>|hidden")
}

// An interpolation immediately followed by a block tag with no text between:
// the `${if}` is mid-line and consumes nothing.
function bt_m3_block_tag_after_interp_greet(name: string, extra: bool) -> string {
    `Hi ${name}${if (extra)}!${endif}`
}

function bt_m3_block_tag_after_interp() -> string {
    bt_m3_block_tag_after_interp_greet("Alice", true) + "|" + bt_m3_block_tag_after_interp_greet("Bob", false)
}

test "backtick_m3_block_tag_directly_after_interp" {
    assert.equal(bt_m3_block_tag_after_interp(), "Hi Alice!|Hi Bob")
}

// `${if (c)}${endif}` with no body content — both branches empty.
function bt_m3_if_empty_true_body() -> string {
    let yes = true
    let no = false
    `[${if (yes)}${endif}|${if (no)}should_not_appear${endif}]`
}

test "backtick_m3_if_empty_true_body" {
    assert.equal(bt_m3_if_empty_true_body(), "[|]")
}

// An entire for-block as the body of an else-branch.
function bt_m3_if_else_branch_for_render(empty: bool, xs: string[]) -> string {
    `${if (empty)}<none>${else}${for (let x in xs)}${x},${endfor}${endif}`
}

function bt_m3_if_else_branch_for() -> string {
    bt_m3_if_else_branch_for_render(false, ["a", "b", "c"]) + "|" + bt_m3_if_else_branch_for_render(true, ["a"])
}

test "backtick_m3_if_else_one_branch_has_for" {
    assert.equal(bt_m3_if_else_branch_for(), "a,b,c,|<none>")
}

// The synthesized accumulator must not capture a user binding of the same
// name: if it did, the inner `${__m3_for}` would see the growing string
// instead of "OUTER".
function bt_m3_for_accumulator_no_capture() -> string {
    let __m3_for = "OUTER"
    let xs = ["a", "b"]
    `${for (let x in xs)}<${x}:${__m3_for}>${endfor}`
}

test "backtick_m3_for_accumulator_does_not_capture_user_binding" {
    assert.equal(bt_m3_for_accumulator_no_capture(), "<a:OUTER><b:OUTER>")
}

// Paren-optional grammar: `${if enabled}` with a bare identifier condition.
function bt_m3_if_bare_cond_gate(enabled: bool) -> string {
    `${if enabled}on${else}off${endif}`
}

function bt_m3_if_bare_cond() -> string {
    bt_m3_if_bare_cond_gate(true) + "|" + bt_m3_if_bare_cond_gate(false)
}

test "backtick_m3_if_no_parens_bare_identifier_cond" {
    assert.equal(bt_m3_if_bare_cond(), "on|off")
}

// ─── M1 delimiter ladder ─────────────────────────────────────────────────────

// §8: a two-tick delimiter allows a single backtick in the content without
// escaping, and the runtime value retains those inner backticks verbatim.
function bt_multi_tick_ladder() -> string {
    ``inline `code` here``
}

test "backtick_multi_tick_ladder_preserves_inner_ticks_at_runtime" {
    assert.equal(bt_multi_tick_ladder(), "inline `code` here")
}

// ─── B-1474: an authored escape is content, never layout ─────────────────────

// The reported bug: `\n` was decoded to a real newline *before* the §12 dedent
// ran, so the dedent's trailing trim ate it and a newline-terminated file came
// out without its newline.
function bt_keeps_trailing_escaped_newline() -> string {
    let host = "alpha"
    `${host}\n`
}

test "backtick_keeps_trailing_escaped_newline" {
    assert.equal(bt_keeps_trailing_escaped_newline(), "alpha\n")
}

function bt_keeps_repeated_trailing_escaped_newlines() -> string {
    `LANG=C\n\n`
}

test "backtick_keeps_repeated_trailing_escaped_newlines" {
    assert.equal(bt_keeps_repeated_trailing_escaped_newlines(), "LANG=C\n\n")
}

// `\n` and `\t` are two source characters each. Nothing may read them as
// layout, so the literal spaces around them survive too.
function bt_escape_not_indentation() -> string {
    ` a\n b\t `
}

test "backtick_escape_does_not_look_like_indentation" {
    assert.equal(bt_escape_not_indentation(), " a\n b\t ")
}

// The same escape on a literal that really is dedented: the shared indent goes,
// the authored newline stays.
function bt_multiline_keeps_trailing_escaped_newline() -> string {
    `
    line one
    line two\n
    `
}

test "backtick_multiline_keeps_trailing_escaped_newline" {
    assert.equal(bt_multiline_keeps_trailing_escaped_newline(), "line one\nline two\n")
}

// Only one line break belongs to the closing delimiter, so a blank line in
// front of it is how you ask for a trailing newline without an escape.
function bt_blank_line_before_closer() -> string {
    `
    line one

    `
}

test "backtick_blank_line_before_closer_is_content" {
    assert.equal(bt_blank_line_before_closer(), "line one\n")
}
