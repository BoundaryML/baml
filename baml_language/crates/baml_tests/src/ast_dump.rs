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
            Item::Class(c) => c.methods.iter().for_each(&mut take),
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
    normalize_source_map(text, &mut rowan, typed);
    rowan == *typed
}

/// Rewrite the rowan quirks in `rowan`'s spans that the typed lowering does
/// not reproduce, where `typed` has the typed span at the same index.
fn normalize_source_map(text: &str, rowan: &mut AstSourceMap, typed: &AstSourceMap) {
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
}

/// [`normalize_source_map`] on every source map an item carries, paired by
/// position with the typed item's.
fn normalize_item(text: &str, rowan: &mut Item, typed: &Item) {
    use baml_compiler2_ast::{DeclarativeMeta, FunctionBodyDef, FunctionDef};
    fn function(text: &str, a: &mut FunctionDef, b: &FunctionDef) {
        if let (Some(FunctionBodyDef::Expr(_, a_map)), Some(FunctionBodyDef::Expr(_, b_map))) =
            (&mut a.body, &b.body)
        {
            normalize_source_map(text, a_map, b_map);
        }
        normalize_source_map(text, &mut a.defaults.source_map, &b.defaults.source_map);
        if let (Some(DeclarativeMeta::Llm(a_llm)), Some(DeclarativeMeta::Llm(b_llm))) =
            (&mut a.declarative_meta, &b.declarative_meta)
        {
            for ((_, (_, a_map)), (_, (_, b_map))) in a_llm
                .companion_bodies
                .iter_mut()
                .zip(&b_llm.companion_bodies)
            {
                normalize_source_map(text, a_map, b_map);
            }
        }
    }
    fn functions(text: &str, a: &mut [FunctionDef], b: &[FunctionDef]) {
        for (a, b) in a.iter_mut().zip(b) {
            function(text, a, b);
        }
    }
    match (rowan, typed) {
        (Item::Function(a), Item::Function(b)) => function(text, a, b),
        (Item::Class(a), Item::Class(b)) => functions(text, &mut a.methods, &b.methods),
        (Item::Interface(a), Item::Interface(b)) => {
            functions(text, &mut a.default_methods, &b.default_methods);
        }
        (Item::ImplementsFor(a), Item::ImplementsFor(b)) => {
            functions(text, &mut a.methods, &b.methods)
        }
        (Item::Let(a), Item::Let(b)) => {
            if let (Some((_, a_map)), Some((_, b_map))) = (&mut a.initializer, &b.initializer) {
                normalize_source_map(text, a_map, b_map);
            }
        }
        // Nothing with a body, or items of two kinds (reported as a difference).
        (Item::Enum(_), _)
        | (Item::TypeAlias(_), _)
        | (Item::Function(_), _)
        | (Item::Class(_), _)
        | (Item::Interface(_), _)
        | (Item::ImplementsFor(_), _)
        | (Item::Let(_), _) => {}
    }
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

/// The same file's items lowered from the typed tree, with the same relative
/// `label` for the `$init_test_<path>` registrar.
fn typed_items(text: &str, label: &str) -> (Vec<Item>, Vec<EnvVarRef>) {
    let parsed = baml_parser::parse(text);
    let (items, _diags, env) =
        baml_compiler2_ast::lower_tree::lower_file(&parsed.tree, Some(Path::new(label)), None);
    (items, env)
}

/// The kind and name an item is reported by.
fn item_label(item: &Item) -> String {
    let name = match item {
        Item::Function(f) => f.name.to_string(),
        Item::Class(c) => c.name.to_string(),
        Item::Enum(e) => e.name.to_string(),
        Item::Interface(i) => i.name.to_string(),
        Item::TypeAlias(t) => t.name.to_string(),
        Item::Let(l) => l.name.to_string(),
        Item::ImplementsFor(imp) => format!(
            "{:?} for {:?}",
            imp.interface_target.kind, imp.for_target.kind
        ),
    };
    format!("{} `{name}`", kind(item))
}

/// Items known to differ, as (file, item label, reason); `*` as the label is
/// every item of the file. Every entry must keep differing; the list may only
/// shrink.
pub const ITEM_PENDING: &[(&str, &str, &str)] = &[
    (
        "projects/diagnostic_errors/trace_hook_placement/main.baml",
        "let `global`",
        "rowan's file-scope `let` docstring keeps a `/// baml:$trace` line (its declaration walk skips them; the `let` walk did not); the typed lowering treats every declaration alike",
    ),
    (
        "projects/diagnostic_errors/unknown_attributes/unknown_attributes.baml",
        "function `helper`",
        "a `@@` before a declaration is a parse error in the new syntax, not the function's attribute (rowan attached it)",
    ),
    (
        "projects/broken_syntax/error_cases/syntax_errors.baml",
        "class `Broken`",
        "an unclosed class ends at the next item head; rowan read `function 123Invalid() {}` as a method of it",
    ),
    (
        "projects/broken_syntax/parser_error_recovery/partial_input.baml",
        "class `Partial`",
        "an unclosed attribute's argument is the next token; rowan included the comment before it in the argument text",
    ),
    (
        "projects/broken_syntax/parser_strings/nested_quotes.baml",
        "class `NestedQuotes`",
        "a string in a recovery position is one token, not re-split around its quotes (parity pending too)",
    ),
    (
        "projects/broken_syntax/pending_greaters_fix/extra_greater.baml",
        "*",
        "a stray `>` recovers at the item head; rowan ran the items together (parity pending too)",
    ),
    (
        "projects/broken_syntax/type_attributes/nested.baml",
        "class `AttributeInsideParenthesizedUnion`",
        "`(A @alias(\"x\") | B)[]`: the attribute is reported and the `[]` kept; rowan dropped the array",
    ),
    (
        "projects/diagnostic_errors/format_checks/config_decls.baml",
        "function `$init_test_projects_diagnostic_errors_format_checks_config_decls`",
        "a block comment between `test` and its name: rowan took the comment as the name (`Missing`); the name is the string",
    ),
];

/// A/B oracle for the typed item lowering: every file lowers to the same
/// items (`Item` equality — `TypeExpr` spans do not take part) and the same
/// environment references from either tree.
///
/// `BAML_ITEM_DUMP=<label>[,<label>…]` (or `*`) prints both sides of the named
/// differing items in full; `BAML_ITEM_DUMP_FILE=<file label>` restricts the
/// run to one file.
#[test]
fn typed_items_match_rowan() {
    let mut report = String::new();
    let mut unexpected_passes = Vec::new();
    let (mut compared, mut differing) = (0usize, 0usize);
    let dump = baml_env::raw_var("BAML_ITEM_DUMP").unwrap_or_default();
    let dumps = |label: &str| dump == "*" || dump.split(',').any(|wanted| wanted == label);
    let only_file = baml_env::raw_var("BAML_ITEM_DUMP_FILE");
    for (label, path) in source_files() {
        if only_file.as_deref().is_some_and(|only| only != label) {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let (rowan_items, rowan_env) = {
            let tokens = baml_compiler_lexer::lex_lossless(&text, FileId::new(0));
            let (green, _parse_errors) = baml_compiler_parser::parse_file(&tokens);
            let root = SyntaxNode::new_root(green);
            let (items, _diagnostics, env) =
                baml_compiler2_ast::lower_file_with_path(&root, Some(Path::new(&label)));
            (items, env)
        };
        let (typed_items, typed_env) = typed_items(&text, &label);
        // An item whose body is on `BODY_PENDING` differs for the body's
        // reason; it is not listed twice. A `*` entry covers the whole file:
        // its items may differ, and at least one must.
        let item_pending = |item: &str| {
            ITEM_PENDING
                .iter()
                .any(|(file, pending, _)| *file == label && *pending == item)
                || BODY_PENDING.iter().any(|(file, function, _)| {
                    *file == label && item == format!("function `{function}`")
                })
        };
        let file_pending = ITEM_PENDING
            .iter()
            .any(|(file, pending, _)| *file == label && *pending == "*");
        let mut file_differs = false;
        let pending_for = |item: &str| item_pending(item) || file_pending;
        let first_diff = |a: &str, b: &str| -> String {
            a.lines()
                .zip(b.lines())
                .enumerate()
                .find(|(_, (x, y))| x != y)
                .map_or_else(
                    || format!("line counts {} vs {}", a.lines().count(), b.lines().count()),
                    |(i, (x, y))| {
                        format!(
                            "line {}:\n    rowan: {x}{}\n    typed: {y}{}",
                            i + 1,
                            span_text(&text, x),
                            span_text(&text, y)
                        )
                    },
                )
        };
        // Items pair up by position; a count difference is reported once.
        let mut rowan_iter = rowan_items.iter();
        let mut typed_iter = typed_items.iter();
        loop {
            match (rowan_iter.next(), typed_iter.next()) {
                (Some(a), Some(b)) => {
                    compared += 1;
                    let item = item_label(a);
                    let pending = pending_for(&item);
                    let mut a = a.clone();
                    normalize_item(&text, &mut a, b);
                    let a = &a;
                    if a == b {
                        if item_pending(&item) {
                            unexpected_passes.push(format!("{label} {item}"));
                        }
                        continue;
                    }
                    file_differs = true;
                    if pending {
                        continue;
                    }
                    differing += 1;
                    // Type spans do not take part in equality; keep them out
                    // of the report so the first difference shown is real.
                    let strip = |dump: String| -> String {
                        dump.lines()
                            .filter(|line| !line.trim_start().starts_with("span: "))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let (x, y) = (format!("{a:#?}"), format!("{b:#?}"));
                    writeln!(
                        report,
                        "{label} {item}: {}",
                        first_diff(&strip(x.clone()), &strip(y.clone()))
                    )
                    .unwrap();
                    if dumps(&item) {
                        writeln!(
                            report,
                            "==== rowan {item} ====\n{x}\n==== typed {item} ====\n{y}"
                        )
                        .unwrap();
                    }
                }
                (Some(a), None) => {
                    compared += 1;
                    file_differs = true;
                    let item = item_label(a);
                    if !pending_for(&item) {
                        differing += 1;
                        writeln!(report, "{label} {item}: only rowan").unwrap();
                    }
                }
                (None, Some(b)) => {
                    compared += 1;
                    file_differs = true;
                    let item = item_label(b);
                    if !pending_for(&item) {
                        differing += 1;
                        writeln!(report, "{label} {item}: only typed").unwrap();
                    }
                }
                (None, None) => break,
            }
        }
        if file_pending && !file_differs {
            unexpected_passes.push(format!("{label} (whole file)"));
        }
        // The typed lowering walks a declaration's members in source order,
        // the rowan one kind by kind, so environment references are compared
        // as a multiset.
        let sorted = |env: &[EnvVarRef]| -> Vec<String> {
            let mut env: Vec<String> = env.iter().map(|e| format!("{e:?}")).collect();
            env.sort();
            env
        };
        if sorted(&rowan_env) != sorted(&typed_env) && !pending_for("env") {
            differing += 1;
            writeln!(
                report,
                "{label} env: {}",
                first_diff(&format!("{rowan_env:#?}"), &format!("{typed_env:#?}"))
            )
            .unwrap();
        }
    }
    assert!(
        unexpected_passes.is_empty(),
        "items on ITEM_PENDING no longer differ; remove them:\n{}",
        unexpected_passes.join("\n")
    );
    assert!(
        report.is_empty(),
        "{differing} of {compared} items differ:\n{report}"
    );
}

/// Diagnostics the rowan pipeline raises from its lowering's file-wide
/// walks and the typed pipeline reports from its parser, where the literal
/// or directive is read: invalid string escapes (`check_string_escapes`) and
/// misplaced, malformed or repeated trace hooks (`reject_unsupported_trace_hooks`,
/// `docstring::trace_hook`). Differences, each with its reason; an entry must
/// keep differing.
pub const DIAG_PENDING: &[(&str, &str)] = &[];

/// The moved diagnostics of the rowan pipeline as `(range, message)`,
/// sorted so the two sides compare as multisets.
fn moved_rowan_diagnostics(text: &str, label: &str) -> Vec<((u32, u32), String)> {
    use baml_compiler2_ast::LoweringDiagnostic as D;
    let tokens = baml_compiler_lexer::lex_lossless(text, FileId::new(0));
    let (green, _parse_errors) = baml_compiler_parser::parse_file(&tokens);
    let root = SyntaxNode::new_root(green);
    let (_items, diagnostics, _env) =
        baml_compiler2_ast::lower_file_with_path(&root, Some(Path::new(label)));
    let mut moved: Vec<((u32, u32), String)> = diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic {
            D::InvalidStringEscape { issue, span } => Some((key(*span), issue.to_string())),
            D::InvalidTraceHook { message, span } => Some((key(*span), message.clone())),
            // Every other kind is raised by both lowerings or by neither.
            _ => None,
        })
        .collect();
    moved.sort();
    moved
}

fn moved_typed_diagnostics(text: &str) -> Vec<((u32, u32), String)> {
    use baml_parser::ParseError as E;
    let mut moved: Vec<((u32, u32), String)> = baml_parser::parse(text)
        .errors
        .iter()
        .filter_map(|error| match error {
            E::InvalidStringEscape { issue, range } => Some((key(*range), issue.to_string())),
            E::InvalidDirective { message, range } => Some((key(*range), message.clone())),
            E::UnexpectedToken { .. }
            | E::UnexpectedEof { .. }
            | E::InvalidSyntax { .. }
            | E::AmbiguousUnion { .. }
            | E::RemovedFeature { .. } => None,
        })
        .collect();
    moved.sort();
    moved
}

/// Every file's moved diagnostics agree between the two pipelines as a
/// multiset of `(range, message)`.
#[test]
fn moved_diagnostics_match_rowan() {
    let mut report = String::new();
    let mut unexpected_passes = Vec::new();
    for (label, path) in source_files() {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        let rowan = moved_rowan_diagnostics(&text, &label);
        let typed = moved_typed_diagnostics(&text);
        let pending = DIAG_PENDING.iter().any(|(file, _)| *file == label);
        if rowan == typed {
            if pending {
                unexpected_passes.push(label);
            }
            continue;
        }
        if pending {
            continue;
        }
        let mut only =
            |side: &str, these: &[((u32, u32), String)], those: &[((u32, u32), String)]| {
                let mut those = those.to_vec();
                for entry in these {
                    if let Some(at) = those.iter().position(|other| other == entry) {
                        those.swap_remove(at);
                    } else {
                        let ((start, end), message) = entry;
                        let _ = writeln!(
                            report,
                            "{label}: only {side} {start}..{end} {message:?}{}",
                            span_text(&text, &format!("{start}..{end}"))
                        );
                    }
                }
            };
        only("rowan", &rowan, &typed);
        only("typed", &typed, &rowan);
    }
    assert!(
        unexpected_passes.is_empty(),
        "files on DIAG_PENDING no longer differ; remove them:\n{}",
        unexpected_passes.join("\n")
    );
    assert!(report.is_empty(), "moved diagnostics differ:\n{report}");
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
