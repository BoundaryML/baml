//! Syntax-layer benchmarks: lex → parse → lower, each stage in isolation.
//!
//! Run with: `cargo bench -p baml_compiler2_ast --bench syntax_benchmark`
//!
//! Every per-file compile step is O(file); this bench pins how that cost is
//! split between the lexer, the parser and CST→AST lowering so that a syntax
//! rewrite can be held to "never slower at any size, and lowering faster".
//! Inputs scale from a realistic file to a stress size no real project reaches:
//!
//! * `stress_x1` — `baml_tests/projects/parser_stress/large_file.baml` as is
//!   (≈7k lines), the largest hand-written v1 file in the repo.
//! * `stress_x8` / `stress_x64` — the same file repeated (≈1 MB / ≈8.5 MB).
//!   Duplicated declarations are irrelevant here: lex, parse and lowering never
//!   look at names, so the repeat only scales the input.
//! * `v0_1mb` / `v0_8mb` — the v0 playground stress fixtures. They use removed
//!   syntax, so they exercise the parser's recovery path on a very large input
//!   rather than the happy path.
//!
//! Each stage is measured with the earlier stages run outside the timed
//! region, so `parse` excludes lexing and `lower` excludes both.

use std::{path::Path, sync::OnceLock};

use baml_base::FileId;
use baml_compiler_syntax::SyntaxNode;
use divan::{Bencher, black_box, counter::BytesCount};
use rowan::GreenNode;

#[expect(
    clippy::print_stderr,
    reason = "a bench binary's only channel for a skip notice"
)]
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping syntax_benchmark in debug/test profile.");
        return;
    }

    divan::main();
}

// ============================================================================
// Inputs
// ============================================================================

/// The input names, in the order they are reported. The v0 fixtures are
/// tracked with git LFS; a checkout without the LFS objects holds a pointer
/// file instead, and those inputs are then left out (with a note) rather
/// than measured or failing every bench.
fn inputs() -> Vec<&'static str> {
    sources().iter().map(|(name, _)| *name).collect()
}

/// The sources, read and scaled once so disk I/O never lands in a sample.
#[expect(
    clippy::print_stderr,
    reason = "the bench's own note about an input it had to skip"
)]
fn sources() -> &'static [(&'static str, String)] {
    static SOURCES: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    SOURCES.get_or_init(|| {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let stress = read(&crates.join("baml_tests/projects/parser_stress/large_file.baml"))
            .unwrap_or_else(|| unreachable!("the stress file is a plain checked-in file"));
        let v0 = crates.join("../../engine/playground-server/tests");
        let mut sources = vec![
            ("stress_x1", stress.clone()),
            ("stress_x8", stress.repeat(8)),
            ("stress_x64", stress.repeat(64)),
        ];
        for (name, file) in [
            (
                "v0_1mb",
                "big-files/baml_src/supply_chain_1mb_test.lfs.baml",
            ),
            (
                "v0_8mb",
                "really-big-files/baml_src/describe_pet_8mb_test.lfs.baml",
            ),
        ] {
            match read(&v0.join(file)) {
                Some(text) => sources.push((name, text)),
                None => {
                    eprintln!("{name}: skipped, {file} is a git LFS pointer (run `git lfs pull`)");
                }
            }
        }
        sources
    })
}

/// The file's text, or `None` for a git LFS pointer standing in for it.
fn read(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    (!text.starts_with("version https://git-lfs")).then_some(text)
}

fn source(name: &str) -> &'static str {
    sources()
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, text)| text.as_str())
        .unwrap_or_else(|| unreachable!("`inputs` names only loaded sources"))
}

// ============================================================================
// Stages
// ============================================================================

fn lex(src: &str) -> Vec<baml_compiler_lexer::Token> {
    baml_compiler_lexer::lex_lossless(src, FileId::new(0))
}

fn parse(tokens: &[baml_compiler_lexer::Token]) -> GreenNode {
    let (green, _errors) = baml_compiler_parser::parse_file(tokens);
    green
}

// ============================================================================
// Benchmarks
// ============================================================================

#[divan::bench(args = inputs())]
fn lex_only(bencher: Bencher, name: &str) {
    let src = source(name);
    bencher
        .counter(BytesCount::of_str(src))
        .bench(|| black_box(lex(black_box(src))));
}

/// The new modal lexer (`baml_syntax::lexer`), for comparison with `lex_only`.
#[divan::bench(args = inputs())]
fn lex_only_typed(bencher: Bencher, name: &str) {
    let src = source(name);
    bencher
        .counter(BytesCount::of_str(src))
        .bench(|| black_box(baml_syntax::lexer::lex(black_box(src))));
}

#[divan::bench(args = inputs())]
fn parse_only(bencher: Bencher, name: &str) {
    let src = source(name);
    let tokens = lex(src);
    bencher
        .counter(BytesCount::of_str(src))
        .bench(|| black_box(parse(black_box(&tokens))));
}

/// The typed parser (`baml_parser::parse`), lexing included: it reads the
/// text straight into the tree, so this row compares with `lex_only` +
/// `parse_only` together.
#[divan::bench(args = inputs())]
fn lex_parse_typed(bencher: Bencher, name: &str) {
    let src = source(name);
    bencher
        .counter(BytesCount::of_str(src))
        .bench(|| black_box(baml_parser::parse(black_box(src))));
}

/// Building the IDE's parent/owner index over a typed tree.
#[divan::bench(args = inputs())]
fn tree_index_typed(bencher: Bencher, name: &str) {
    let src = source(name);
    let tree = baml_parser::parse(src).tree;
    bencher
        .counter(BytesCount::of_str(src))
        .bench(|| black_box(baml_syntax::index::TreeIndex::build(black_box(&tree))));
}

/// The red `SyntaxNode` is `Rc`-based and so `!Sync`, which divan's timed
/// closure requires; the `Arc`-based green tree is shared instead and the red
/// root is rebuilt per sample in the untimed input step.
#[divan::bench(args = inputs())]
fn lower_only(bencher: Bencher, name: &str) {
    let src = source(name);
    let green = parse(&lex(src));
    bencher
        .counter(BytesCount::of_str(src))
        .with_inputs(|| SyntaxNode::new_root(green.clone()))
        .bench_refs(|root| black_box(baml_compiler2_ast::lower_file(root)));
}

/// The whole per-file syntax pipeline, as a compile or an LSP edit pays it —
/// including building the red root, which lowering needs and a compile pays.
#[divan::bench(args = inputs())]
fn lex_parse_lower(bencher: Bencher, name: &str) {
    let src = source(name);
    bencher.counter(BytesCount::of_str(src)).bench(|| {
        let root = SyntaxNode::new_root(parse(&lex(black_box(src))));
        black_box(baml_compiler2_ast::lower_file(&root))
    });
}
