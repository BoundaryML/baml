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
