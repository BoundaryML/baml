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
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use baml_base::FileId;
use baml_compiler2_ast::{
    EnvVarRef,
    ast::{AstSourceMap, ExprBody, Item},
};
use baml_db::{baml_compiler_lexer, baml_compiler_parser, baml_compiler_syntax::SyntaxNode};
use text_size::TextRange;

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

/// The user-written function bodies of a file as the rowan lowering produced
/// them, keyed by the span of the function's name: every function with an
/// expression body it did not synthesize (so no LLM function, companion,
/// registrar or `cleanup` guard), wherever it is declared.
fn rowan_bodies(text: &str, label: &str) -> BTreeMap<(u32, u32), (ExprBody, AstSourceMap)> {
    use baml_compiler2_ast::{FunctionBodyDef, FunctionDef, FunctionOrigin, Item};
    let tokens = baml_compiler_lexer::lex_lossless(text, FileId::new(0));
    let (green, _parse_errors) = baml_compiler_parser::parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    let (items, _diagnostics, _env) =
        baml_compiler2_ast::lower_file_with_path(&root, Some(Path::new(label)));
    let mut bodies = BTreeMap::new();
    let mut take = |f: &FunctionDef| {
        if f.metadata.origin != FunctionOrigin::UserDefined
            || f.declarative_meta.is_some()
            || f.name.as_str() == "cleanup"
        {
            return;
        }
        if let Some(FunctionBodyDef::Expr(body, map)) = &f.body {
            bodies.insert(key(f.name_span), (body.clone(), map.clone()));
        }
    };
    for item in &items {
        match item {
            Item::Function(f) => take(f),
            Item::Class(c) => {
                c.methods.iter().for_each(&mut take);
                c.implements
                    .iter()
                    .flat_map(|b| &b.methods)
                    .for_each(&mut take);
            }
            Item::Interface(i) => i.default_methods.iter().for_each(&mut take),
            Item::ImplementsFor(imp) => imp.methods.iter().for_each(&mut take),
            Item::Enum(_) | Item::TypeAlias(_) | Item::Let(_) => {}
        }
    }
    bodies
}

/// The source text a Debug line's `a..b` names, for the report.
fn span_text(text: &str, line: &str) -> String {
    let digits: Vec<&str> = line
        .trim()
        .trim_end_matches(',')
        .trim_start_matches("span: ")
        .split("..")
        .collect();
    let [a, b] = digits[..] else {
        return String::new();
    };
    match (a.parse::<usize>(), b.parse::<usize>()) {
        (Ok(a), Ok(b)) if a <= b && b <= text.len() => format!("  {:?}", &text[a..b]),
        _ => String::new(),
    }
}

/// Whether two source maps agree, allowing one difference the typed tree
/// makes on purpose: the rowan pattern of a `let`/`const`/`for` binding
/// spanned its introducer too; the typed pattern spans the pattern alone.
fn source_maps_agree(text: &str, rowan: &AstSourceMap, typed: &AstSourceMap) -> bool {
    let mut rowan = rowan.clone();
    // A single-segment callee with type args, `foo<T>(x)`, spanned the `<T>`
    // in rowan (a multi-segment one did not); the typed callee is the path.
    if rowan.expr_spans.len() == typed.expr_spans.len() {
        for (a, b) in rowan.expr_spans.iter_mut().zip(typed.expr_spans.iter()) {
            let (a_span, b_span) = (*a.1, *b.1);
            if a_span.start() == b_span.start() && a_span.end() > b_span.end() {
                let tail = &text[b_span.end().into()..a_span.end().into()];
                if tail.starts_with('<') && tail.ends_with('>') {
                    *a.1 = b_span;
                }
            }
        }
    }
    // A statement's span reached over the trivia to its `;`.
    if rowan.stmt_spans.len() == typed.stmt_spans.len() {
        for (a, b) in rowan.stmt_spans.iter_mut().zip(typed.stmt_spans.iter()) {
            let (a_span, b_span) = (*a.1, *b.1);
            if a_span.start() == b_span.start()
                && a_span.end() > b_span.end()
                && text[b_span.end().into()..a_span.end().into()]
                    .trim_end()
                    .strip_suffix(';')
                    .is_some_and(trivia_only)
            {
                *a.1 = b_span;
            }
        }
    }
    // A `let` pattern's span started at the introducer.
    if rowan.pattern_spans.len() == typed.pattern_spans.len() {
        for (a, b) in rowan
            .pattern_spans
            .iter_mut()
            .zip(typed.pattern_spans.iter())
        {
            let (a_span, b_span) = (*a.1, *b.1);
            if a_span.start() < b_span.start() && a_span.end() == b_span.end() {
                let lead = &text[a_span.start().into()..b_span.start().into()];
                let after_introducer = lead
                    .strip_prefix("let")
                    .or_else(|| lead.strip_prefix("const"));
                if after_introducer.is_some_and(|rest| !rest.is_empty() && trivia_only(rest)) {
                    *a.1 = b_span;
                }
            }
        }
    }
    rowan == *typed
}

/// Whitespace and comments only.
fn trivia_only(mut s: &str) -> bool {
    loop {
        s = s.trim_start();
        if s.is_empty() {
            return true;
        }
        if let Some(rest) = s.strip_prefix("//") {
            s = rest.split_once('\n').map_or("", |(_, rest)| rest);
        } else if let Some(rest) = s.strip_prefix("/*") {
            let Some((_, rest)) = rest.split_once("*/") else {
                return false;
            };
            s = rest;
        } else {
            return false;
        }
    }
}

/// A range as a map key (`TextRange` is not `Ord`).
fn key(range: TextRange) -> (u32, u32) {
    (range.start().into(), range.end().into())
}

/// The same bodies lowered from the typed tree.
fn typed_bodies(text: &str) -> BTreeMap<(u32, u32), (ExprBody, AstSourceMap)> {
    use baml_syntax::nodes::{AnyFunctionBody, FunctionDef};
    let parsed = baml_parser::parse(text);
    let tree = &parsed.tree;
    let mut bodies = BTreeMap::new();
    for id in tree.arenas().ids::<FunctionDef>() {
        let f = tree.get(id);
        let (Ok(name), Ok(AnyFunctionBody::Expr(block))) = (f.name, f.body) else {
            continue;
        };
        let name_span = tree.tokens().range(tree.get(name).token.idx);
        let (body, map, _diags, _env) = baml_compiler2_ast::lower_tree::lower_body(tree, block);
        bodies.insert(key(name_span), (body, map));
    }
    bodies
}

/// Bodies known to differ, as (file, function, reason). Every entry must
/// keep differing; the list may only shrink.
pub const BODY_PENDING: &[(&str, &str, &str)] = &[
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenAssignSimple",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenCompoundAssign",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenAssignInLet",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenSafeAssignField",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenSafeAssignIndex",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenSafeCompoundAssign",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenSafeAssignReturn",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenSafeAssignDeepChain",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "ParenSafeAssignMapIndex",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/diagnostic_errors/assignment_expr_position/main.baml",
        "assignment_in_expression_position",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/diagnostic_errors/null_handling/null_handling.baml",
        "OptionalChaining",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/diagnostic_errors/null_handling/null_handling.baml",
        "OptionalIndex",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "projects/diagnostic_errors/null_handling/null_handling.baml",
        "Combined",
        "`(x = 5)` in expression position: the parser reports it and keeps `x`; rowan lowered the whole assignment to `Missing`",
    ),
    (
        "corpus/ns_unit_type/unit_type.baml",
        "unit_header_after_value",
        "ruled 2026-11-04: a header after the last expression makes it a statement, so the block has no value (rowan kept the value); the fixture becomes a diagnostic case at the cutover",
    ),
    (
        "projects/broken_syntax/banned_expressions/banned_expressions.baml",
        "Foo",
        "`let a = let b = 42`: the inner `let` is a statement of its own, not an assignment to `b`",
    ),
    (
        "projects/broken_syntax/braceless_lambda/braceless_lambda.baml",
        "Braceless2",
        "the braceless lambda `(x: int) => x + ;` ends at the `+`, not at the `;`",
    ),
    (
        "projects/broken_syntax/error_cases/syntax_errors.baml",
        "Invalid",
        "`function 123Invalid`: the name is missing, not `Invalid` after the number",
    ),
    (
        "projects/broken_syntax/mismatched_brackets/main.baml",
        "ExtraCloseParen",
        "a stray closer is a reported statement of its own, so the call before it is not the tail",
    ),
    (
        "projects/broken_syntax/mismatched_brackets/main.baml",
        "UnclosedGeneric",
        "unbalanced-bracket recovery: the leftover tokens are a reported statement",
    ),
    (
        "projects/broken_syntax/mismatched_brackets/main.baml",
        "ExtraCloseAngle",
        "unbalanced-bracket recovery: each stray `>` is a reported statement",
    ),
    (
        "projects/broken_syntax/mismatched_brackets/main.baml",
        "MismatchedParenBracket",
        "a stray closer is a reported statement of its own, so the expression before it is not the tail",
    ),
    (
        "projects/broken_syntax/mismatched_brackets/main.baml",
        "UnclosedArray",
        "an unclosed `[` stops at the next item head; rowan read `function` as an element",
    ),
    (
        "projects/broken_syntax/parser_error_recovery/invalid_syntax.baml",
        "Foo",
        "`client: GPT4` in a body with a missing return type: `client` is a statement and `: GPT4` a reported one",
    ),
    (
        "projects/broken_syntax/parser_speculative/ambiguous_function.baml",
        "AnotherLLM",
        "`prompt: `…`` after a misspelled field is a path, a reported `:` and an untagged template, not a tagged one",
    ),
    (
        "projects/broken_syntax/patterns_array_rest_parser_recovery/patterns_array_rest_parser_recovery.baml",
        "spread_token_is_not_rest",
        "`[...let r]`: the binding survives the reported `...`; rowan left a wildcard and a type",
    ),
    (
        "projects/broken_syntax/patterns_array_rest_parser_recovery/patterns_array_rest_parser_recovery.baml",
        "rest_missing_binding_name",
        "`[..let]`: a rest with no binding has no sub-pattern; rowan made a wildcard",
    ),
    (
        "projects/broken_syntax/patterns_array_rest_parser_recovery/patterns_array_rest_parser_recovery.baml",
        "rest_bare_ascription",
        "`[.. : int[]]`: the type is read as a suffix pattern; rowan made a wildcard rest and an empty array",
    ),
    (
        "projects/broken_syntax/removed_scoped_type_syntax/removed_scoped_type_syntax.baml",
        "removed_statement",
        "a removed `type T = …` statement is reported and dropped, so `T` is unbound after it (ruled 2026-11-04: accept the cascade; rowan kept a binding)",
    ),
    (
        "projects/broken_syntax/removed_scoped_type_syntax/removed_scoped_type_syntax.baml",
        "removed_body_aliases",
        "a removed `type T = …` statement is reported and dropped, so `T` is unbound after it (ruled 2026-11-04: accept the cascade; rowan kept a binding)",
    ),
    (
        "projects/broken_syntax/type_attributes/bindings.baml",
        "attribute_on_the_right_hand_side",
        "the `@alias(\"x\")` after a `let type` is a reported statement; rowan dropped it",
    ),
    (
        "projects/parser_stress/very_invalid.baml",
        "BodyTorture",
        "`let x = ¿;` and `while \\ {`: the unlexable bytes recover as reported statements, in a different shape",
    ),
];

/// A/B oracle for the typed body lowering: every user-written function body
/// lowers to the same arenas and source map from either tree.
///
/// `BAML_BODY_DUMP=<name>[,<name>…]` (or `*`) prints both sides of the named
/// differing bodies in full.
#[test]
fn typed_bodies_match_rowan() {
    let mut report = String::new();
    let mut unexpected_passes = Vec::new();
    let (mut compared, mut differing) = (0usize, 0usize);
    let dump = baml_env::raw_var("BAML_BODY_DUMP").unwrap_or_default();
    let dumps = |name: &str| dump == "*" || dump.split(',').any(|wanted| wanted == name);
    for (label, path) in source_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let rowan = rowan_bodies(&text, &label);
        let typed = typed_bodies(&text);
        for (span, (body, map)) in &rowan {
            compared += 1;
            let name = &text[span.0 as usize..span.1 as usize];
            let pending = BODY_PENDING
                .iter()
                .any(|(file, function, _)| *file == label && *function == name);
            let difference = match typed.get(span) {
                None => Some("no typed body".to_string()),
                Some((typed_body, typed_map))
                    if typed_body != body || !source_maps_agree(&text, map, typed_map) =>
                {
                    // Type spans do not take part in equality; keep them out
                    // of the report so the first difference shown is real.
                    let strip = |dump: String| -> String {
                        dump.lines()
                            .filter(|line| !line.trim_start().starts_with("span: "))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let a = format!("{}\n{map:#?}", strip(format!("{body:#?}")));
                    let b = format!("{}\n{typed_map:#?}", strip(format!("{typed_body:#?}")));
                    let first = a
                        .lines()
                        .zip(b.lines())
                        .enumerate()
                        .find(|(_, (x, y))| x != y)
                        .map_or_else(
                            || {
                                format!(
                                    "line counts {} vs {}",
                                    a.lines().count(),
                                    b.lines().count()
                                )
                            },
                            |(i, (x, y))| {
                                format!(
                                    "line {}:\n    rowan: {x}{}\n    typed: {y}{}",
                                    i + 1,
                                    span_text(&text, x),
                                    span_text(&text, y)
                                )
                            },
                        );
                    if dumps(name) {
                        writeln!(
                            report,
                            "==== rowan `{name}` ====\n{a}\n==== typed `{name}` ====\n{b}"
                        )
                        .unwrap();
                    }
                    Some(first)
                }
                Some(_) => None,
            };
            match (difference, pending) {
                (Some(difference), false) => {
                    differing += 1;
                    writeln!(report, "{label} `{name}`: {difference}").unwrap();
                }
                (Some(_), true) => {}
                (None, true) => unexpected_passes.push(format!("{label} `{name}`")),
                (None, false) => {}
            }
        }
    }
    assert!(
        unexpected_passes.is_empty(),
        "bodies on BODY_PENDING no longer differ; remove them:\n{}",
        unexpected_passes.join("\n")
    );
    assert!(
        report.is_empty(),
        "{differing} of {compared} bodies differ:\n{report}"
    );
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
