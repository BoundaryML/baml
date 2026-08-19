//! Backtick-literal tests whose BAML source must contain *raw bytes* that a
//! `.baml` file cannot safely hold.
//!
//! The rest of the backtick suite lives in
//! `crates/baml_tests/baml_src/ns_backtick_strings/` as native `test` blocks —
//! see `baml_language/TEST_INSTRUCTIONS.md`. These three cannot go there.
//!
//! Each one asserts something about how the *lexer* treats a byte that repo
//! tooling normalizes away in a checked-in source file:
//!   - a CRLF pair and a lone CR inside a literal (both must lex to `\n`);
//!   - three trailing spaces after a `${for}` tag, which must still count as
//!     "alone on line".
//!
//! Written as a `.baml` file those bytes are load-bearing file content, and two
//! separate tools rewrite them: git's `*.baml text eol=lf` attribute strips the
//! CR on checkin, and the `trailing-whitespace` pre-commit hook eats the
//! spaces. Either rewrite turns the test into a vacuous pass rather than a
//! failure. Here the BAML source is a Rust string literal, so `\r` and
//! `   \n` are escapes — the bytes are produced at runtime and never appear in
//! this file, which is why these are stable.

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};

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
        expected: Ok(BexExternalValue::String("a\nb\nend".into())),
        ..Default::default()
    })
    .await
}
