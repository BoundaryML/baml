//! End-to-end runtime tests for BEP-049 backtick string literals.
//!
//! These compile real BAML source through the full pipeline and execute
//! it on `BexEngine`, then check the returned value. They are intentionally
//! distinct from the AST-level tests in `baml_compiler2_ast`: those verify the
//! lowered HIR shape; these verify that the bytecode actually evaluates to
//! the expected string at runtime.
//!
//! M1: contents are plain text (no `${...}` interpolation yet — M2).

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

#[tokio::test]
async fn backtick_one_liner_evaluates_to_string() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `hello world`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hello world".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_multiline_dedents_at_runtime() -> anyhow::Result<()> {
    // §12: multi-line content auto-dedents via the shared `dedent_backtick`
    // helper. The 8-space indent on the content lines is stripped, and so are
    // the line break after the opener and the one (plus indent) before the
    // closer — those belong to the delimiters, not to the content.
    assert_engine_executes(EngineProgram {
        source: "
            function main() -> string {
                `
                    line one
                    line two
                `
            }
        ",
        entry: "main",
        expected: Ok(BexExternalValue::String("line one\nline two".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_escapes_decoded_at_runtime() -> anyhow::Result<()> {
    // `\\n` becomes a real newline; `\\`` becomes a literal backtick;
    // `\\${` becomes a literal `${` (M1 — no interpolation yet).
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a\nb\`c\${d}e`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("a\nb`c${d}e".into())),
        ..Default::default()
    })
    .await
}

// ── M2: ${...} interpolation runtime tests ────────────────────────────────

#[tokio::test]
async fn backtick_simple_interpolation_evaluates_at_runtime() -> anyhow::Result<()> {
    // The flagship M2 case: bind a value, interpolate it, get the joined
    // string at runtime.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let name = "world"
                `Hello, ${name}!`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("Hello, world!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_multiple_interpolations_evaluate_at_runtime() -> anyhow::Result<()> {
    // Several interpolations + literal text mixed.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let first = "ada"
                let last = "lovelace"
                `${first} ${last} writes code`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("ada lovelace writes code".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_block_body_with_let_renders_tail_expression() -> anyhow::Result<()> {
    // BEP §4: ${...} is a block expression — statements + optional trailing
    // expression. The block's tail value is what gets interpolated.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `result: ${ let prefix = "hi "; let name = "there"; prefix + name }!`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("result: hi there!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_calls_function() -> anyhow::Result<()> {
    // The interpolated block can call user-defined functions.
    assert_engine_executes(EngineProgram {
        source: r#"
            function shout(s: string) -> string {
                s + "!!!"
            }
            function main() -> string {
                `Loud: ${shout("hi")} and ${shout("bye")}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("Loud: hi!!! and bye!!!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_with_loop_in_block_body() -> anyhow::Result<()> {
    // Cursed but legal: a while loop builds a value inside `${...}` and the
    // final expression is what gets interpolated. Exercises the block-body
    // grammar (statements + tail expr) and proves the lowering doesn't
    // assume the body is a trivial expression.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
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
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("loop result: xxx!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_with_let_bound_if_else_expression() -> anyhow::Result<()> {
    // BEP §4 worked example: bind an if-expression's value with a let, then
    // interpolate it. (The bare `${if …}` form also works now — see
    // `backtick_inline_if_expression_renders` below.)
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let logged_in = true
                `welcome ${ let r = if (logged_in) { "user" } else { "guest" }; r }`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("welcome user".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_inline_if_expression_renders() -> anyhow::Result<()> {
    // BEP §4 README:205 — `${if (cond) { "pos" } else { "neg" }}` is a valid
    // inline if-EXPRESSION render. A runtime condition keeps the branches as a
    // literal-union (`"pos" | "neg"`); interpolating it resolves `.to_string()`
    // through the union (the probe arm) and MIR dispatches it as one direct
    // primitive call (no class-tag map read).
    assert_engine_executes(EngineProgram {
        source: r#"
            function classify(x: int) -> string {
                `${if (x > 0) { "pos" } else { "neg" }}`
            }
            function main() -> string {
                classify(5) + "/" + classify(-1)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("pos/neg".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_inline_if_int_union_renders() -> anyhow::Result<()> {
    // Same as above for an int literal-union (`1 | 2`): the runtime value is an
    // int, so `${…}` dispatches to `Int.to_string`.
    assert_engine_executes(EngineProgram {
        source: r#"
            function pick(x: int) -> string {
                `${if (x > 0) { 1 } else { 2 }}`
            }
            function main() -> string {
                pick(5) + "/" + pick(-1)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("1/2".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_cross_site_let_leaks_to_later_segment() -> anyhow::Result<()> {
    // BEP §4 README:213 — a side-effect-only `${ let … }` defines a binding for
    // the rest of the template (the `{% set %}` equivalent). The untagged
    // template lowers into one shared concat scope so the `let` is visible to a
    // later `${…}`. Previously this compiled clean then ICE'd at runtime lowering
    // (`Ty::Unknown` for the later reference).
    assert_engine_executes(EngineProgram {
        source: r#"
            function f(items: int[]) -> string {
                `count: ${ let n = items.length() }${n} items`
            }
            function main() -> string {
                f([1, 2, 3])
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("count: 3 items".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_void_tail_renders_empty() -> anyhow::Result<()> {
    // BEP §4 README:194 — a block whose tail evaluates to unit renders "". An
    // `if` without `else` is void-typed; it must render "" rather than error on a
    // missing `to_string`.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a${ if (false) { "x" } }b`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("ab".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_nested_in_interpolation() -> anyhow::Result<()> {
    // A backtick string inside an interpolation inside another backtick string.
    // Tests that the parser correctly handles balanced braces and that
    // segments/lowering re-enter cleanly.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let inner = `inner-${"x"}`
                `outer[${inner}]`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("outer[inner-x]".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_with_recursive_call() -> anyhow::Result<()> {
    // The interp body can call a recursive function — exercises that the
    // interp expression isn't restricted to "simple" forms.
    assert_engine_executes(EngineProgram {
        source: r#"
            function bangs(n: int) -> string {
                if (n <= 0) { "" } else { "!" + bangs(n - 1) }
            }
            function main() -> string {
                `done${bangs(5)}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("done!!!!!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_int_auto_to_string() -> anyhow::Result<()> {
    // BEP §11: ${int} works without an explicit `.to_string()` because
    // the lowering wraps each interp with `.to_string()` and `Int` has
    // a stdlib `to_string` method.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let count = 42
                `count: ${count}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("count: 42".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_bool_auto_to_string() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let ok = true
                `status: ${ok}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("status: true".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_float_auto_to_string() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let pi = 3.14
                `pi ~ ${pi}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("pi ~ 3.14".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interpolation_mixed_types_auto_to_string() -> anyhow::Result<()> {
    // Mix of int, bool, float, string in one backtick — every interp gets
    // .to_string() at lowering, regardless of source type.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let n = 7
                let ok = false
                let r = 1.5
                let name = "test"
                `n=${n}, ok=${ok}, r=${r}, name=${name}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String(
            "n=7, ok=false, r=1.5, name=test".into(),
        )),
        ..Default::default()
    })
    .await
}

// ── M2 coverage tests (A–Q) — Tier 1 (suspicious cases) + Tier 2 (assurance) ──

// Cases A, B, E (null / class-without-to_string / optional interpolation)
// are deliberately strict-typed per BEP §7 ("surfaces null-handling bugs in
// prompts at type-check time instead of at LLM-call time") and §11
// (implicit `.to_string()` dispatch). They produce a compile error rather
// than silently rendering "null" or similar. Compile-error tests live in
// the fixture pipeline: `baml_tests/projects/diagnostic_errors/backtick_strict_types/`.

// (C) Empty interpolation `${}` — block with no stmts, no tail → unit value.
// Per BEP §4 unit renders as "". Currently `unit.to_string()` likely fails.
#[tokio::test]
async fn backtick_case_c_empty_interp_renders_empty() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a${}b`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("ab".into())),
        ..Default::default()
    })
    .await
}

// (D) Statement-only block `${ let x = 5 }` — block ends in a statement,
// no tail. Per BEP §4 unit → "". Likely fails for same reason as (C).
#[tokio::test]
async fn backtick_case_d_statement_only_block_renders_empty() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a${ let x = 5 }b`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("ab".into())),
        ..Default::default()
    })
    .await
}

// BEP-049 §4 cross-site `let`: a `let` bound in a side-effect-only
// `${ let … }` segment is visible to a later `${…}` in the same template.
// The untagged desugaring splices statement-only segments into ONE shared
// concat scope (like the `${for}` accumulator), rather than confining each to
// its own block — so `w` resolves across segments. Regression for an ICE
// ("an error-recovery type reached runtime lowering"): `w` used to fail name
// resolution, type `Ty::Unknown`, dodge both interp diagnostics, and panic at
// MIR runtime lowering.
#[tokio::test]
async fn backtick_cross_site_let_binds_into_concat_scope() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `${ let w = "hi" }${w}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hi".into())),
        ..Default::default()
    })
    .await
}

// Cross-site `let` with surrounding text and a second binding that depends on
// the first — confirms later segments observe earlier segments' bindings in
// source order, and that intervening text segments don't break visibility.
#[tokio::test]
async fn backtick_cross_site_let_chain_with_text() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a${ let x = "X" }b${x}c${ let y = x + "Y" }d${y}e`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("abXcdXYe".into())),
        ..Default::default()
    })
    .await
}

// Side-effect evaluation order is preserved when statement-only segments are
// hoisted into the shared concat block: a `let` reassigned across segments
// reflects each mutation at the point the next value segment reads it.
#[tokio::test]
async fn backtick_cross_site_let_reassign_evaluation_order() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let n = 0
                `${ n = n + 1 }${n}-${ n = n + 1 }${n}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("1-2".into())),
        ..Default::default()
    })
    .await
}

// (E) covered by the same strict-types fixture as (A) and (B).
// See `baml_tests/projects/diagnostic_errors/backtick_strict_types/`.

// (F) Adjacent interpolations `${a}${b}` with no text between.
#[tokio::test]
async fn backtick_case_f_adjacent_interpolations() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let a = "ab"
                let b = "cd"
                `${a}${b}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("abcd".into())),
        ..Default::default()
    })
    .await
}

// (G) Interpolation at the very start (no leading text).
#[tokio::test]
async fn backtick_case_g_interp_at_start() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let x = "hi"
                `${x}!`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hi!".into())),
        ..Default::default()
    })
    .await
}

// (H) Interpolation at the very end (no trailing text).
#[tokio::test]
async fn backtick_case_h_interp_at_end() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let x = "hi"
                `!${x}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("!hi".into())),
        ..Default::default()
    })
    .await
}

// (I) Multi-tick delimiter + interpolation combined.
#[tokio::test]
async fn backtick_case_i_multi_tick_with_interpolation() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let x = "Y"
                ``X ${x} Z``
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("X Y Z".into())),
        ..Default::default()
    })
    .await
}

// (J) Custom class controlling its rendering via `implements baml.ToString` —
// `${p}` renders through `string.from`, which dispatches to the override.
#[tokio::test]
async fn backtick_case_j_user_class_with_to_string() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            class Person {
                name string
                implements baml.ToString {
                    function to_string(self) -> string throws never {
                        self.name
                    }
                }
            }
            function main() -> string {
                let p = Person { name: "Ada" }
                `meet ${p}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("meet Ada".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_interp_class_without_to_string_renders_structurally() -> anyhow::Result<()> {
    // §11: a class that does NOT implement `baml.ToString` is still
    // interpolatable — `${p}` renders via `string.from`, which falls back to a
    // structural rendering of the instance.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Point {
                x int
                y int
            }
            function main() -> string {
                let p = Point { x: 1, y: 2 }
                `p = ${p}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("p = Point { x: 1, y: 2 }".into())),
        ..Default::default()
    })
    .await
}

// (K) Backtick string as the default value of a function parameter.
#[tokio::test]
async fn backtick_case_k_default_parameter_value() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function greet(name: string, prefix: string = `Hello, `) -> string {
                prefix + name
            }
            function main() -> string {
                greet("world")
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("Hello, world".into())),
        ..Default::default()
    })
    .await
}

// (L) Backtick used as an expression-statement (value discarded).
#[tokio::test]
async fn backtick_case_l_expression_statement() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> int {
                `unused`;
                42
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::Int(42)),
        ..Default::default()
    })
    .await
}

// (M) Two backtick strings concatenated with `+`.
#[tokio::test]
async fn backtick_case_m_two_backticks_concatenated() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a` + `b`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("ab".into())),
        ..Default::default()
    })
    .await
}

// (N) Backtick literal passed as an argument to a function call.
#[tokio::test]
async fn backtick_case_n_passed_as_function_argument() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function shout(s: string) -> string { s + "!" }
            function main() -> string {
                let name = "Ada"
                shout(`Hello ${name}`)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("Hello Ada!".into())),
        ..Default::default()
    })
    .await
}

// (O) Array indexing inside `${...}` body.
#[tokio::test]
async fn backtick_case_o_array_index_in_interp() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let arr = ["a", "b", "c"]
                `first: ${arr[0]}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("first: a".into())),
        ..Default::default()
    })
    .await
}

// (P) Method chain inside `${...}` on a let-bound value.
#[tokio::test]
async fn backtick_case_p_method_chain_on_let_bound() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let s = "hello"
                `upper: ${ let r = s.to_upper_case(); r }`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("upper: HELLO".into())),
        ..Default::default()
    })
    .await
}

// (Q) Throw inside `${...}` body — should propagate out of the backtick.
#[tokio::test]
async fn backtick_case_q_throw_in_interp_body() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string throws string {
                `x: ${ throw "boom" }`
            }
        "#,
        entry: "main",
        expected: Err("boom"),
        ..Default::default()
    })
    .await
}

// ── TypeScript-parity tests (AA, BB, GG, HH, LL) ──────────────────────────

// (AA) CR/CRLF normalization in backtick text. Mirrors TS scanner behavior.
#[tokio::test]
async fn backtick_crlf_normalization() -> anyhow::Result<()> {
    // Source contains a literal `\r\n` between "line1" and "line2"; the
    // value should contain `\n` only.
    assert_engine_executes(EngineProgram {
        source: "function main() -> string {\n    `line1\r\nline2`\n}\n",
        entry: "main",
        expected: Ok(BexExternalValue::String("line1\nline2".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_lone_cr_normalization() -> anyhow::Result<()> {
    // Bare CR (old-Mac line ending) — normalized to LF.
    assert_engine_executes(EngineProgram {
        source: "function main() -> string {\n    `line1\rline2`\n}\n",
        entry: "main",
        expected: Ok(BexExternalValue::String("line1\nline2".into())),
        ..Default::default()
    })
    .await
}

// (BB) Extended C-style escapes: \b, \v, \f.
#[tokio::test]
async fn backtick_extended_escape_backspace() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a\bb`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("a\u{0008}b".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_extended_escape_vertical_tab() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a\vb`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("a\u{000B}b".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_extended_escape_form_feed() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `a\fb`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("a\u{000C}b".into())),
        ..Default::default()
    })
    .await
}

// (GG) Block-comment inside `${...}` body.
#[tokio::test]
async fn backtick_block_comment_inside_interp() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let name = "Ada"
                `hi ${ /* greet someone */ name }`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hi Ada".into())),
        ..Default::default()
    })
    .await
}

// (HH) Line-comment inside `${...}` body.
#[tokio::test]
async fn backtick_line_comment_inside_interp() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let name = "Ada"
                `hi ${
                    // a friendly greeting
                    name
                }`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("hi Ada".into())),
        ..Default::default()
    })
    .await
}

// (LL) `$` at the very end of a template — must stay literal (no `{` follows).
#[tokio::test]
async fn backtick_trailing_dollar_is_literal() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `cost: $`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("cost: $".into())),
        ..Default::default()
    })
    .await
}

// ── M3 block control flow runtime tests ───────────────────────────────────

#[tokio::test]
async fn backtick_m3_for_loop_basic() -> anyhow::Result<()> {
    // BEP §5 block-tag form. The for-loop iterates over `xs` and each
    // iteration's text + interpolation gets concatenated into the output.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs = ["a", "b", "c"]
                `${for (let x in xs)}- ${x}
${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("- a\n- b\n- c\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_loop_with_text_around() -> anyhow::Result<()> {
    // Text before and after the for-block flows in order.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs = ["x", "y"]
                `header
${for (let v in xs)}${v}, ${endfor}done`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("header\nx, y, done".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_block_tag_true_branch() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let logged_in = true
                `${if (logged_in)}welcome${endif}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("welcome".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_block_tag_false_branch_empty() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let logged_in = false
                `${if (logged_in)}welcome${endif}!`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_else_chain() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function classify(n: int) -> string {
                `${if (n > 0)}pos${else if (n < 0)}neg${else}zero${endif}`
            }
            function main() -> string {
                classify(5) + " " + classify(-3) + " " + classify(0)
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("pos neg zero".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_nested_for_in_if() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs = ["a", "b"]
                let show = true
                `${if (show)}${for (let x in xs)}- ${x}
${endfor}${endif}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("- a\n- b\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_with_interp_in_body() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs = [1, 2, 3]
                `${for (let x in xs)}${x},${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("1,2,3,".into())),
        ..Default::default()
    })
    .await
}

// ── M3 §13 whitespace-control worked cases from the BEP ───────────────────

#[tokio::test]
async fn backtick_m3_ws_case1_items_on_own_lines() -> anyhow::Result<()> {
    // BEP §13 Case 1: `${for}` and `${endfor}` alone on their lines, body
    // line indented. Each iter emits the indented body + trailing newline.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let names = ["a", "b", "c"]
                `
${for (let n in names)}
  - ${n}
${endfor}
`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("  - a\n  - b\n  - c\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case2_inline_concatenation() -> anyhow::Result<()> {
    // BEP §13 Case 2: all tags inline — mid-line, so consume nothing.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let names = ["a", "b", "c"]
                `${for (let n in names)}${n}${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("abc".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case3_blank_line_between_iterations() -> anyhow::Result<()> {
    // BEP §13 Case 3: a blank line inside the body becomes a separator.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let names = ["a", "b", "c"]
                `
${for (let n in names)}
- ${n}

${endfor}
`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("- a\n\n- b\n\n- c\n\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case6_if_full_line_block() -> anyhow::Result<()> {
    // BEP §13 Case 6: full-line `${if}` / `${endif}` consume their own lines
    // entirely, so the false branch leaves no extra newline behind.
    let src = r#"
        function classify(extra: bool) -> string {
            `
Header
${if (extra)}
Extra
${endif}
Footer
`
        }
        function main() -> string {
            classify(true) + "|" + classify(false)
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        // The line break before the closing backtick is the closer's own, so
        // "Footer\n" lands as "Footer" in the output.
        expected: Ok(BexExternalValue::String(
            "Header\nExtra\nFooter|Header\nFooter".into(),
        )),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case7_if_introduces_content_only_when_true() -> anyhow::Result<()> {
    // BEP §13 Case 7: `${if}` mid-line — newline after `${if}` is body
    // content, not block-tag whitespace, so it shows up only when true.
    let src = r#"
        function show(extra: bool) -> string {
            `
Header${if (extra)}
Extra line${endif}
Footer
`
        }
        function main() -> string {
            show(true) + "|" + show(false)
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String(
            "Header\nExtra line\nFooter|Header\nFooter".into(),
        )),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case8_nested_loops_preserving_structure() -> anyhow::Result<()> {
    // BEP §13 Case 8: nested `${for}` loops, each alone on its line.
    assert_engine_executes(EngineProgram {
        source: r#"
            class Group {
                name string
                items string[]
            }
            function main() -> string {
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
        "#,
        entry: "main",
        // The trailing newline is the inner for-body's last `\n`, emitted
        // by each outer iteration — it's body content, not block-tag ws.
        expected: Ok(BexExternalValue::String(
            "Group: Fruits\n  - apple\n  - banana\nGroup: Drinks\n  - coffee\n".into(),
        )),
        ..Default::default()
    })
    .await
}

// ── M3 edge cases (gap-filling) ──────────────────────────────────────────

#[tokio::test]
async fn backtick_m3_for_loop_over_empty_collection() -> anyhow::Result<()> {
    // Zero iterations → accumulator stays empty. The accumulator-init
    // string is the result; surrounding text still flows through.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs: string[] = []
                `before|${for (let x in xs)}- ${x}
${endfor}|after`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("before||after".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_loop_empty_body() -> anyhow::Result<()> {
    // A for-block with nothing between open and close tags should
    // contribute the empty string, even with N iterations.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs = [1, 2, 3]
                `[${for (let _x in xs)}${endfor}]`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("[]".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_loop_over_array_literal() -> anyhow::Result<()> {
    // The collection expression is an inline array literal — exercises
    // the `lower_expr` path for the collection (vs bare-token shortcut).
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `${for (let n in [10, 20, 30])}${n} ${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("10 20 30 ".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_cstyle_for_basic() -> anyhow::Result<()> {
    // BEP §4: the C-style `for (init; cond; step)` header is accepted in
    // `${for}` (same headers as the host `for`), not just the iterator form.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `${for (let i = 0; i < 3; i += 1)}[${i}]${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("[0][1][2]".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_cstyle_for_bound_by_outer_local() -> anyhow::Result<()> {
    // The C-style header reads an enclosing local (`n`) in its condition, and
    // the body interpolates the loop counter.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let n = 4
                `${for (let i = 0; i < n; i += 1)}${i},${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("0,1,2,3,".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_cstyle_for_step_by_two() -> anyhow::Result<()> {
    // A non-unit step.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `${for (let i = 0; i < 6; i += 2)}${i} ${endfor}`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("0 2 4 ".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_nested_if_in_for() -> anyhow::Result<()> {
    // If-chain inside for-body — both block-tag forms composed.
    // Uses an outer-scope flag so the if condition doesn't have to
    // resolve against the for-binding's element type. (The for-binding's
    // type doesn't currently reach the if condition through HIR — that's
    // a TIR-level inference gap unrelated to M3 lowering.)
    let src = r#"
        function go(flag: bool, xs: string[]) -> string {
            `${for (let x in xs)}${if (flag)}+${x}${else}-${x}${endif} ${endfor}`
        }
        function main() -> string {
            go(true, ["a", "b"]) + "|" + go(false, ["a", "b"])
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("+a +b |-a -b ".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case4_blank_line_after_block() -> anyhow::Result<()> {
    // BEP §13 Case 4: blank line between the for-block and a trailing
    // Footer paragraph. The blank line stays in the output.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let names = ["a", "b", "c"]
                `
${for (let n in names)}
- ${n}
${endfor}

Footer
`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("- a\n- b\n- c\n\nFooter".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_case5_conditional_inline() -> anyhow::Result<()> {
    // BEP §13 Case 5: mid-line `${if}` — consumes nothing.
    let src = r#"
        function greet(formal: bool) -> string {
            `Hello${if (formal)}, sir${endif}!`
        }
        function main() -> string {
            greet(true) + "|" + greet(false)
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("Hello, sir!|Hello!".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_mid_line_indented_for_keeps_indent_literal() -> anyhow::Result<()> {
    // BEP §13 "mid-line edge case": indented `${for}` followed by inline
    // body+endfor+Suffix on the same line is mid-line — the leading 4
    // spaces stay in the output as literal text. `enumerate` style not
    // used; the BEP's exact example uses a join-like body.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let names = ["a", "b", "c"]
                `
Prefix:
    ${for (let n in names)}${n} ${endfor}Suffix
`
            }
        "#,
        entry: "main",
        // 4 spaces before `${for}` are not consumed because the line
        // contains other content (`${n} ${endfor}Suffix`).
        expected: Ok(BexExternalValue::String("Prefix:\n    a b c Suffix".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_tag_at_literal_start() -> anyhow::Result<()> {
    // No preceding text at all — the literal-start position is
    // treated as start-of-line, so a `${for}` at offset 0 is alone if
    // its trailing-to-newline check passes.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let xs = ["x", "y"]
                `${for (let n in xs)}
${n}
${endfor}done`
            }
        "#,
        entry: "main",
        // ForOpen alone (start-of-literal + immediate \n) → strips
        // trailing \n. Endfor alone → strips trailing... but `done`
        // follows on same line (no \n before `done`), so Endfor is
        // mid-line and consumes nothing.
        expected: Ok(BexExternalValue::String("x\ny\ndone".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_loop_shadows_outer_let() -> anyhow::Result<()> {
    // Outer `let x` should not be visible inside the for-body —
    // the for-binding shadows it for the duration of the loop.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let x = "OUTER"
                let xs = ["a", "b"]
                let inner = `${for (let x in xs)}${x},${endfor}`
                inner + "|" + x
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("a,b,|OUTER".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_with_complex_condition() -> anyhow::Result<()> {
    // `&&` / `||` / comparison in the condition.
    let src = r#"
        function f(a: int, b: int) -> string {
            `${if (a > 0 && b > 0)}pos${else if (a < 0 || b < 0)}neg${else}zero${endif}`
        }
        function main() -> string {
            f(1, 1) + "|" + f(-1, 0) + "|" + f(0, 0)
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("pos|neg|zero".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_ws_trailing_spaces_after_tag_alone() -> anyhow::Result<()> {
    // Trailing spaces on the same line as the tag (before the newline)
    // still count as alone-on-line — the rule allows whitespace up to
    // the next newline.
    assert_engine_executes(EngineProgram {
        source: "
            function main() -> string {
                let xs = [\"a\", \"b\"]
                `
${for (let n in xs)}   \n${n}
${endfor}
end`
            }
        ",
        entry: "main",
        // The trailing 3 spaces + \n after `${for}` are all consumed
        // (alone on line). Same for `${endfor}`. Each iter emits "X\n".
        // The literal-end "end" remains since `${endfor}` was alone
        // and consumed its own line, then `end` is on the next line.
        expected: Ok(BexExternalValue::String("a\nb\nend".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_inside_if_branch() -> anyhow::Result<()> {
    // The opposite of nested-for-in-if: an if-block whose true branch
    // contains a for-loop. Exercises the recursive lowering on if-bodies.
    let src = r#"
        function render(show: bool, xs: string[]) -> string {
            `${if (show)}${for (let x in xs)}<${x}>${endfor}${else}hidden${endif}`
        }
        function main() -> string {
            render(true, ["a", "b"]) + "|" + render(false, ["a", "b"])
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("<a><b>|hidden".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_block_tag_directly_after_interp() -> anyhow::Result<()> {
    // `${name}${if (extra)}...${endif}` — interp immediately followed by
    // a block tag with no text between. The if is mid-line, so consumes
    // nothing. Validates the §13 rule isn't fooled by adjacent non-text.
    let src = r#"
        function greet(name: string, extra: bool) -> string {
            `Hi ${name}${if (extra)}!${endif}`
        }
        function main() -> string {
            greet("Alice", true) + "|" + greet("Bob", false)
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("Hi Alice!|Hi Bob".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_empty_true_body() -> anyhow::Result<()> {
    // `${if (c)}${endif}` with no body content — both branches empty.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let yes = true
                let no = false
                `[${if (yes)}${endif}|${if (no)}should_not_appear${endif}]`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("[|]".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_else_one_branch_has_for() -> anyhow::Result<()> {
    // The false branch contains a for-loop. Exercises lowering an
    // entire for-block as the body of an else-branch.
    let src = r#"
        function render(empty: bool, xs: string[]) -> string {
            `${if (empty)}<none>${else}${for (let x in xs)}${x},${endfor}${endif}`
        }
        function main() -> string {
            render(false, ["a", "b", "c"]) + "|" + render(true, ["a"])
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("a,b,c,|<none>".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_for_accumulator_does_not_capture_user_binding() -> anyhow::Result<()> {
    // The synthesized accumulator name must not collide with a user
    // binding of the same name — `${user_var}` inside the body should
    // resolve to the OUTER user value, not the inner accumulator.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let __m3_for = "OUTER"
                let xs = ["a", "b"]
                `${for (let x in xs)}<${x}:${__m3_for}>${endfor}`
            }
        "#,
        entry: "main",
        // If the accumulator captures the user's `__m3_for`, the inner
        // `${__m3_for}` would see the growing string instead of "OUTER".
        expected: Ok(BexExternalValue::String("<a:OUTER><b:OUTER>".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_m3_if_no_parens_bare_identifier_cond() -> anyhow::Result<()> {
    // Paren-optional grammar: `${if enabled}` with a bare identifier as the
    // condition. Previously the if-lowering's children-only scan skipped
    // bare-token conditions and silently fell through to `Expr::Missing`.
    let src = r#"
        function gate(enabled: bool) -> string {
            `${if enabled}on${else}off${endif}`
        }
        function main() -> string {
            gate(true) + "|" + gate(false)
        }
    "#;
    assert_engine_executes(EngineProgram {
        source: src,
        entry: "main",
        expected: Ok(BexExternalValue::String("on|off".into())),
        ..Default::default()
    })
    .await
}

// ── M1 tests retained below ───────────────────────────────────────────────

#[tokio::test]
async fn backtick_multi_tick_ladder_preserves_inner_ticks_at_runtime() -> anyhow::Result<()> {
    // §8: two-tick delimiter allows a single backtick in content without
    // escaping. The runtime value should retain those inner backticks verbatim.
    assert_engine_executes(EngineProgram {
        source: "
            function main() -> string {
                ``inline `code` here``
            }
        ",
        entry: "main",
        expected: Ok(BexExternalValue::String("inline `code` here".into())),
        ..Default::default()
    })
    .await
}

// ── B-1474: an authored escape is content, never layout ───────────────────

#[tokio::test]
async fn backtick_keeps_trailing_escaped_newline() -> anyhow::Result<()> {
    // The reported bug: `\n` was decoded to a real newline *before* the §12
    // dedent ran, so the dedent's trailing trim ate it and a
    // newline-terminated file came out without its newline.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                let host = "alpha"
                `${host}\n`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("alpha\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_keeps_repeated_trailing_escaped_newlines() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `LANG=C\n\n`
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("LANG=C\n\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_escape_does_not_look_like_indentation() -> anyhow::Result<()> {
    // `\n` and `\t` are two source characters each. Nothing may read them as
    // layout, so the literal spaces around them survive too.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                ` a\n b\t `
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String(" a\n b\t ".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_multiline_keeps_trailing_escaped_newline() -> anyhow::Result<()> {
    // Same escape, this time on a literal that really is dedented: the 16-space
    // indent goes, the authored newline stays.
    assert_engine_executes(EngineProgram {
        source: r#"
            function main() -> string {
                `
                line one
                line two\n
                `
            }
        "#,
        entry: "main",
        expected: Ok(BexExternalValue::String("line one\nline two\n".into())),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_blank_line_before_closer_is_content() -> anyhow::Result<()> {
    // Only one line break belongs to the closing delimiter, so a blank line in
    // front of it is how you ask for a trailing newline without an escape.
    assert_engine_executes(EngineProgram {
        source: "
            function main() -> string {
                `
                line one

                `
            }
        ",
        entry: "main",
        expected: Ok(BexExternalValue::String("line one\n".into())),
        ..Default::default()
    })
    .await
}
