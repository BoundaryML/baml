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
    // §12: multi-line content auto-dedents via the shared `preprocess_template`
    // helper. The 8-space indent on the content lines is stripped, the leading
    // newline after the opener is consumed, and the trailing whitespace before
    // the closer is trimmed.
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
    // BEP §4 worked example, adapted to use a let inside the interp so the
    // .to_string() wrap targets a path expression, not a bare if-expr.
    //
    // Background: `${if (cond) {...} else {...}}` would directly call
    // .to_string() on the inline if-expression result, which trips a
    // pre-existing VM lowering issue ("expected map, got string"). The
    // BEP-compliant escape hatch is to bind the value first — the block-
    // expression body grammar lets users do this naturally.
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

// (J) Custom class WITH a `to_string` method — implicit dispatch should hit it.
#[tokio::test]
async fn backtick_case_j_user_class_with_to_string() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
            class Person {
                name string
                function to_string(self) -> string {
                    self.name
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
