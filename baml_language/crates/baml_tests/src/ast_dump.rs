//! Per-file rendering of the lowered AST, for comparing two lowerings.
//!
//! The syntax-layer rebuild runs a second parser and lowering beside the
//! current ones. Their equivalence oracle is an in-process comparison of this
//! rendering for both pipelines over every `.baml` file the repo owns — there
//! is deliberately no checked-in golden, because a file hashing every lowered
//! item would be rewritten by every AST or span change and conflict on every
//! grammar PR. Diagnostics are excluded: the `05_diagnostics` snapshots
//! compare those.
//!
//! The rendering is `Debug`, spans included, grouped by item kind so a
//! difference can be gated per kind while a port lands one kind at a time.
//! `AstSourceMap` keeps its collections ordered (`BTreeMap`/`BTreeSet`) so
//! this rendering is the same in every process.
//!
//! For a manual before/after diff, run the ignored test with
//! `BAML_AST_DUMP_DIR=<dir>` on each commit and diff the two directories.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use baml_base::FileId;
use baml_compiler2_ast::{EnvVarRef, ast::Item};
use baml_db::{baml_compiler_lexer, baml_compiler_parser, baml_compiler_syntax::SyntaxNode};

/// Every tree of `.baml` sources this repo owns, as `(label, dir)`; the label
/// prefixes each file so a rendering reads the same on every machine.
const ROOTS: [(&str, &str); 4] = [
    ("corpus", "baml_src"),
    ("projects", "projects"),
    ("stdlib", "../baml_builtins2/baml_std"),
    ("type_quiz", "../../tools/type_quiz"),
];

/// Item kinds in rendering order. [`kind`] is an exhaustive match over
/// [`Item`], so a new variant fails to compile there; its name must then be
/// added here too, or the first item of that kind panics in `Rendered::new`.
pub const KINDS: [&str; 7] = [
    "function",
    "class",
    "enum",
    "interface",
    "type_alias",
    "let",
    "implements_for",
];

fn kind(item: &Item) -> &'static str {
    match item {
        Item::Function(_) => "function",
        Item::Class(_) => "class",
        Item::Enum(_) => "enum",
        Item::Interface(_) => "interface",
        Item::TypeAlias(_) => "type_alias",
        Item::Let(_) => "let",
        Item::ImplementsFor(_) => "implements_for",
    }
}

/// One file's lowering, rendered per item kind (source order within a kind)
/// plus its environment-variable references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// `(kind, rendering)` in [`KINDS`] order; a kind with no items renders
    /// as the empty string.
    pub by_kind: Vec<(&'static str, String)>,
    pub env: String,
}

impl Rendered {
    pub fn new(items: &[Item], env_var_refs: &[EnvVarRef]) -> Self {
        let mut by_kind: Vec<(&'static str, String)> =
            KINDS.iter().map(|k| (*k, String::new())).collect();
        for item in items {
            let slot = by_kind
                .iter_mut()
                .find(|(k, _)| *k == kind(item))
                .unwrap_or_else(|| unreachable!("KINDS lists every `kind` result"));
            writeln!(slot.1, "{item:#?}").unwrap_or_else(|_| unreachable!("String never fails"));
        }
        Self {
            by_kind,
            env: format!("{env_var_refs:#?}"),
        }
    }

    /// The whole rendering as one text, kinds separated by headers.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        for (k, rendered) in &self.by_kind {
            if !rendered.is_empty() {
                writeln!(text, "==== {k}\n{rendered}")
                    .unwrap_or_else(|_| unreachable!("String never fails"));
            }
        }
        writeln!(text, "==== env\n{}", self.env)
            .unwrap_or_else(|_| unreachable!("String never fails"));
        text
    }
}

/// Every `.baml` file under [`ROOTS`] as `(label/relative path, absolute
/// path)`, sorted by label. Hidden directories are skipped: a stray
/// `.baml/cache` left by a CLI run is not source.
pub fn source_files() -> Vec<(String, PathBuf)> {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for (label, dir) in ROOTS {
        let root = crate_dir.join(dir);
        let walk = walkdir::WalkDir::new(&root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| !entry.file_name().to_string_lossy().starts_with('.'));
        for entry in walk {
            let entry = entry.unwrap_or_else(|e| panic!("walking {}: {e}", root.display()));
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("baml") {
                continue;
            }
            let rel = path
                .strip_prefix(&root)
                .unwrap_or_else(|_| unreachable!("walkdir yields paths under its root"))
                .to_string_lossy()
                .replace('\\', "/");
            files.push((format!("{label}/{rel}"), path.to_path_buf()));
        }
    }
    files.sort();
    files
}

/// Lower `text` through the current (rowan) pipeline. The relative `label`,
/// not the absolute path, feeds the per-file `$init_test_<path>` synthesis so
/// the rendering is machine-independent.
pub fn lower_rowan(text: &str, label: &str) -> Rendered {
    let tokens = baml_compiler_lexer::lex_lossless(text, FileId::new(0));
    let (green, _parse_errors) = baml_compiler_parser::parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    let (items, _diagnostics, env_var_refs) =
        baml_compiler2_ast::lower_file_with_path(&root, Some(Path::new(label)));
    Rendered::new(&items, &env_var_refs)
}

/// Write every file's rendering to `$BAML_AST_DUMP_DIR/<label>.ast.txt`.
/// Run on two commits and diff the directories to see exactly which item a
/// lowering change moved.
#[test]
#[ignore = "manual tool: set BAML_AST_DUMP_DIR=<dir> and run with `--ignored` (nextest: `--run-ignored all`)"]
fn ast_dump() {
    let dir = baml_env::os_var("BAML_AST_DUMP_DIR")
        .map(PathBuf::from)
        .expect("BAML_AST_DUMP_DIR names the directory to write the dumps into");
    for (label, path) in source_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let out = dir.join(format!("{label}.ast.txt"));
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("creating {}: {e}", parent.display()));
        }
        std::fs::write(&out, lower_rowan(&text, &label).to_text())
            .unwrap_or_else(|e| panic!("writing {}: {e}", out.display()));
    }
}
