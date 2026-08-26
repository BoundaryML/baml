//! `//^^^ ty` annotation checks and `check_infer`-style dumps, after
//! rust-analyzer's `check_types` / `check_infer`
//! (`crates/hir-ty/src/tests.rs` there), run DIFFERENTIALLY against both
//! engines: `baml_compiler2_hir_ty` (the one being built) and TIR (the one
//! being replaced). The annotation check runs per engine so a fixture can
//! encode which engine satisfies the spec; the dump merges both engines per
//! node so every snapshot is a live diff of the two systems.
//!
//! A fixture is a single BAML file. Annotation lines are comments whose first
//! non-space content after `//` is a caret run; the carets select a byte range
//! on the nearest preceding non-annotation line, and the rest of the line is
//! the expected type in `Ty::render_canonical` syntax:
//!
//! ```text
//! function f() -> int {
//!     let x = 1;
//!      // ^ int
//!     x
//! //  ^ int
//! }
//! ```
//!
//! A caret range matches an expression span or a binding-pattern span. The
//! check is strict in both directions: an annotation that matches nothing
//! fails, an annotation whose type differs fails, and a fixture with no
//! annotations fails. For TIR, error-severity diagnostics also fail (the
//! hir_ty engine has no diagnostics yet; the plan's S17 adds them and the
//! gate). Annotation lines are ordinary comments to the compiler, so ranges
//! are computed against the exact text that gets compiled. Fixtures must be
//! ASCII (caret columns are byte offsets).
//!
//! hir_ty's ERROR CHANNEL is asserted too (rust-analyzer's
//! `check_infer_with_mismatches` discipline): the channel is CLEAN BY
//! DEFAULT - any recorded `type_mismatches` entry or non-exhaustive match
//! without a matching annotation fails the fixture - and a fixture typing
//! an intentionally-invalid program states its errors executable:
//!
//! ```text
//!     g(n)
//! //  ^^^^ mismatch: expected user.Need, got int
//! //  ^^^^ non-exhaustive
//! ```
//!
//! Both are hir_ty-only assertions (TIR's gate stays its diagnostics), and
//! the dump gains a trailing `[mismatch]` / `[non-exhaustive]` section so
//! snapshots show the error channel evolving alongside the types. S17 turns
//! these recorded entries into rendered diagnostics; the assertion surface
//! is already here.
//!
//! The dump side renders one line per inferred node, sorted by range:
//! `start..end 'text': ty` where the engines agree, and
//! `start..end 'text': hir_ty=[..] tir=[..]` where they differ (with
//! `<missing>` for an engine that inferred nothing there). Counting
//! difference lines across snapshots is the progress metric; at cutover the
//! only ones left should be the spec-mandated improvements.
//!
//! Not yet supported (add when a fixture needs it): multi-file fixtures,
//! `|` continuation lines, `^file` whole-file annotations, top-level `let`
//! bodies, and parameter annotations (parameters are not `PatId`-backed).

use std::{collections::BTreeMap, fmt::Write as _};

use baml_compiler2_ast::AstSourceMap;
use baml_compiler2_hir::{
    body::OwnerBody,
    scope::{FileScopeId, ScopeKind},
    semantic_index::FileSemanticIndex,
};
use baml_compiler2_hir_ty::infer::{InferenceResult, infer_body};
use baml_db::ProjectDatabase;
use text_size::{TextRange, TextSize};

use crate::engine::TestDbExt;

/// One caret annotation: the source range it selects, the expectation, and
/// the 1-based line of the annotated code (for error messages).
struct Annotation {
    range: TextRange,
    kind: AnnotationKind,
    expected: String,
    code_line: usize,
}

/// What a caret asserts. `Ty` checks the inferred type (both engines).
/// `Mismatch` and `NonExhaustive` assert hir_ty's ERROR CHANNEL
/// (rust-analyzer's `check_infer_with_mismatches` discipline): the channel
/// is CLEAN BY DEFAULT - every recorded entry must be annotated, every
/// annotation must match a recorded entry - so a fixture typing an invalid
/// program states its expected errors executable, and a spurious mismatch
/// on a valid program goes red. TIR's error gate stays its diagnostics
/// (`tir: fails`); these two kinds are hir_ty-only.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AnnotationKind {
    Ty,
    /// `// ^ mismatch: expected <ty>, got <ty>`
    Mismatch,
    /// `// ^ non-exhaustive`
    NonExhaustive,
}

/// Per-engine annotation-check results plus the merged infer-dump.
pub(crate) struct DifferentialOutcome {
    pub(crate) hir_ty: Result<(), String>,
    pub(crate) dump: String,
}

/// Runs both engines over `fixture` (as `test.baml`), checks the caret
/// annotations against each, and renders the merged dump.
pub(crate) fn run_differential(fixture: &str) -> DifferentialOutcome {
    assert!(fixture.is_ascii(), "fixtures must be ASCII");
    let annotations = extract_annotations(fixture);
    assert!(
        !annotations.is_empty(),
        "fixture has no //^ annotations; check_types requires at least one"
    );

    let mut db = crate::compiler2_tir::support::make_db();
    let file = db.file("test.baml", fixture);

    let hir_ty_nodes = collect_hir_ty_nodes(&db, file, fixture);
    let channel = collect_hir_ty_error_channel(&db, file);

    let mut hir_ty_failures = check_annotations(&hir_ty_nodes, fixture, &annotations);
    hir_ty_failures.extend(check_error_channel(&channel, fixture, &annotations));

    let mut dump = render_infer(&hir_ty_nodes, fixture);
    dump.push_str(&render_error_channel(&channel, fixture));

    DifferentialOutcome {
        hir_ty: to_result(hir_ty_failures),
        dump,
    }
}

/// hir_ty's recorded error channel for one file: type mismatches (rendered
/// `expected X, got Y`, rust-analyzer's wording) and non-exhaustive
/// matches, each at its expression's source range.
pub(crate) struct ErrorChannel {
    pub(crate) mismatches: BTreeMap<(u32, u32), Vec<String>>,
    pub(crate) non_exhaustive: Vec<TextRange>,
}

pub(crate) fn collect_hir_ty_error_channel(
    db: &ProjectDatabase,
    file: baml_base::SourceFile,
) -> ErrorChannel {
    let mut mismatches: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    let mut non_exhaustive = Vec::new();
    for owner in baml_compiler2_ppir::file_body_owners(db, file) {
        let body = baml_compiler2_ppir::body(db, owner);
        if body.expr_body().is_none() {
            continue;
        }
        let Some(source_map) = baml_compiler2_ppir::body_source_map(db, owner) else {
            continue;
        };
        let result = infer_body(db, owner);
        for (&expr_id, (expected, actual)) in &result.type_mismatches {
            let rendered = format!(
                "expected {}, got {}",
                expected.to_plain().render_canonical(),
                actual.to_plain().render_canonical()
            );
            let entry = mismatches
                .entry(range_key(source_map.expr_span(expr_id)))
                .or_default();
            if !entry.contains(&rendered) {
                entry.push(rendered);
            }
        }
        for &expr_id in &result.non_exhaustive_matches {
            let range = source_map.expr_span(expr_id);
            if !non_exhaustive.contains(&range) {
                non_exhaustive.push(range);
            }
        }
    }
    non_exhaustive.sort_by_key(|range| range_key(*range));
    ErrorChannel {
        mismatches,
        non_exhaustive,
    }
}

/// The clean-by-default contract, strict in both directions: every
/// `mismatch:` / `non-exhaustive` annotation must match a recorded entry at
/// its exact range, and every recorded entry must be annotated - an
/// UNANNOTATED entry on a supposedly-valid fixture is exactly the silent
/// disagreement this channel check exists to catch.
fn check_error_channel(
    channel: &ErrorChannel,
    fixture: &str,
    annotations: &[Annotation],
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut covered_mismatches: Vec<(u32, u32)> = Vec::new();
    let mut covered_non_exhaustive: Vec<(u32, u32)> = Vec::new();
    for ann in annotations {
        match ann.kind {
            AnnotationKind::Ty => {}
            AnnotationKind::Mismatch => match channel.mismatches.get(&range_key(ann.range)) {
                None => failures.push(format!(
                    "line {}: no mismatch recorded at `{}`; annotation expects `{}`",
                    ann.code_line, &fixture[ann.range], ann.expected
                )),
                Some(rendered) if !rendered.contains(&ann.expected) => failures.push(format!(
                    "line {}: mismatch at `{}` is `{}`, annotation expects `{}`",
                    ann.code_line,
                    &fixture[ann.range],
                    rendered.join("` / `"),
                    ann.expected
                )),
                Some(_) => covered_mismatches.push(range_key(ann.range)),
            },
            AnnotationKind::NonExhaustive => {
                if channel
                    .non_exhaustive
                    .iter()
                    .any(|range| range_key(*range) == range_key(ann.range))
                {
                    covered_non_exhaustive.push(range_key(ann.range));
                } else {
                    failures.push(format!(
                        "line {}: no non-exhaustive match recorded at `{}`",
                        ann.code_line, &fixture[ann.range]
                    ));
                }
            }
        }
    }
    for (&key, rendered) in &channel.mismatches {
        if !covered_mismatches.contains(&key) {
            let (start, end) = key;
            failures.push(format!(
                "unannotated mismatch at {start}..{end} `{}`: {}; annotate with `mismatch: ...`                  or fix the engine",
                ellipsize(&fixture[start as usize..end as usize], 30),
                rendered.join("` / `")
            ));
        }
    }
    for range in &channel.non_exhaustive {
        if !covered_non_exhaustive.contains(&range_key(*range)) {
            let (start, end) = range_key(*range);
            failures.push(format!(
                "unannotated non-exhaustive match at {start}..{end} `{}`; annotate with                  `non-exhaustive` or fix the engine",
                ellipsize(&fixture[start as usize..end as usize], 30)
            ));
        }
    }
    failures
}

/// The dump's trailing error-channel section (rust-analyzer's
/// `check_infer_with_mismatches` shape): one line per recorded entry, so
/// snapshots show the error channel evolving alongside the types.
fn render_error_channel(channel: &ErrorChannel, fixture: &str) -> String {
    let mut out = String::new();
    for (&(start, end), rendered) in &channel.mismatches {
        let text = ellipsize(
            &fixture[TextRange::new(TextSize::new(start), TextSize::new(end))],
            15,
        );
        for entry in rendered {
            let _ = writeln!(out, "{start}..{end} '{text}': [mismatch] {entry}");
        }
    }
    for range in &channel.non_exhaustive {
        let (start, end) = range_key(*range);
        let text = ellipsize(
            &fixture[TextRange::new(TextSize::new(start), TextSize::new(end))],
            15,
        );
        let _ = writeln!(out, "{start}..{end} '{text}': [non-exhaustive]");
    }
    out
}

/// Checks `fixture`'s annotations against the hir_ty engine only, panicking
/// on failure. Inline-test entry point; fixtures use the runner.
#[track_caller]
#[allow(dead_code, reason = "inline-test entry point; fixtures use the runner")]
pub(crate) fn check_types(fixture: &str) {
    if let Err(report) = run_differential(fixture).hir_ty {
        panic!("check_types failed:\n  {report}");
    }
}

fn to_result(failures: Vec<String>) -> Result<(), String> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n  "))
    }
}

/// Checks every annotation against one engine's typed nodes.
fn check_annotations(
    nodes: &[TypedNode],
    fixture: &str,
    annotations: &[Annotation],
) -> Vec<String> {
    let mut types: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    for node in nodes {
        let entry = types.entry(range_key(node.range)).or_default();
        if !entry.contains(&node.ty) {
            entry.push(node.ty.clone());
        }
    }

    let mut failures = Vec::new();
    for ann in annotations {
        if ann.kind != AnnotationKind::Ty {
            continue;
        }
        match types.get(&range_key(ann.range)) {
            None => {
                let mut msg = format!(
                    "line {}: no inferred type at {:?} (`{}`); expected `{}`",
                    ann.code_line, ann.range, &fixture[ann.range], ann.expected
                );
                write_candidates_on_line(&mut msg, fixture, &types, ann);
                failures.push(msg);
            }
            Some(rendered) if !rendered.contains(&ann.expected) => {
                failures.push(format!(
                    "line {}: `{}` expected `{}`, inferred `{}`",
                    ann.code_line,
                    &fixture[ann.range],
                    ann.expected,
                    rendered.join("` / `")
                ));
            }
            Some(_) => {}
        }
    }
    failures
}

/// Merges both engines' nodes into one dump line per source range:
/// `start..end 'text': ty` on agreement, `hir_ty=[..] tir=[..]` on
/// difference. The name-narrowed binding entries used by caret matching are
/// excluded: the dump reflects real node spans only.
fn render_infer(hir_ty: &[TypedNode], fixture: &str) -> String {
    let mut merged: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    for node in hir_ty {
        if node.kind == NodeKind::BindingName {
            continue;
        }
        let list = merged.entry(range_key(node.range)).or_default();
        if !list.contains(&node.ty) {
            list.push(node.ty.clone());
        }
    }

    let mut out = String::new();
    for (&(start, end), tys) in &mut merged {
        tys.sort();
        let text = ellipsize(
            &fixture[TextRange::new(TextSize::new(start), TextSize::new(end))],
            15,
        );
        for ty in tys.iter() {
            let _ = writeln!(out, "{start}..{end} '{text}': {ty}");
        }
    }
    out
}

/// Shortens `text` to at most `max` bytes with a `...` midsection, escaping
/// newlines so every dump entry stays on one line.
fn ellipsize(text: &str, max: usize) -> String {
    debug_assert!(max >= 5);
    let flat = text.replace('\n', "\\n");
    if flat.len() <= max {
        return flat;
    }
    let prefix = (max - 3) / 2;
    let suffix = max - 3 - prefix;
    format!("{}...{}", &flat[..prefix], &flat[flat.len() - suffix..])
}

fn range_key(range: TextRange) -> (u32, u32) {
    (range.start().into(), range.end().into())
}

/// Lists the ranges on the annotated line that DO have types, so a
/// misaligned caret fails with the nearby candidates instead of nothing.
fn write_candidates_on_line(
    msg: &mut String,
    fixture: &str,
    types: &BTreeMap<(u32, u32), Vec<String>>,
    ann: &Annotation,
) {
    let line_start = fixture[..u32::from(ann.range.start()) as usize]
        .rfind('\n')
        .map_or(0, |idx| idx + 1);
    let line_end = fixture[line_start..]
        .find('\n')
        .map_or(fixture.len(), |idx| line_start + idx);
    let mut candidates = types
        .range((line_start as u32, 0)..(line_end as u32, u32::MAX))
        .peekable();
    if candidates.peek().is_some() {
        let _ = write!(msg, "; typed ranges on that line:");
        for (&(start, end), rendered) in candidates {
            let _ = write!(
                msg,
                " `{}`: `{}`",
                &fixture[start as usize..end as usize],
                rendered.join("` / `")
            );
        }
    }
}

/// Extracts caret annotations. Stacked annotation lines all target the same
/// (nearest preceding non-annotation) line.
fn extract_annotations(text: &str) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    // (byte offset, byte length, 1-based line number) of the target line.
    let mut target: Option<(usize, usize, usize)> = None;
    let mut offset = 0usize;
    for (idx, line) in text.split('\n').enumerate() {
        if let Some((caret_col, caret_len, content)) = parse_annotation_line(line) {
            let (t_offset, t_len, t_line) = target.unwrap_or_else(|| {
                panic!("line {}: annotation has no preceding code line", idx + 1)
            });
            assert!(
                caret_col + caret_len <= t_len,
                "line {}: carets extend past the annotated line {}",
                idx + 1,
                t_line
            );
            let start = t_offset + caret_col;
            let (kind, expected) = if let Some(rest) = content.strip_prefix("mismatch:") {
                (AnnotationKind::Mismatch, rest.trim().to_owned())
            } else if content == "non-exhaustive" {
                (AnnotationKind::NonExhaustive, String::new())
            } else {
                (AnnotationKind::Ty, content.to_owned())
            };
            annotations.push(Annotation {
                range: TextRange::new(
                    TextSize::new(start as u32),
                    TextSize::new((start + caret_len) as u32),
                ),
                kind,
                expected,
                code_line: t_line,
            });
        } else {
            target = Some((offset, line.len(), idx + 1));
        }
        offset += line.len() + 1;
    }
    annotations
}

/// Parses `  // ^^^ expected` into (caret start column, caret run length,
/// expected text). Returns `None` for non-annotation lines. Columns are byte
/// offsets into the whole line so they align with the code line above.
fn parse_annotation_line(line: &str) -> Option<(usize, usize, &str)> {
    let rest = line.trim_start().strip_prefix("//")?;
    let after = rest.trim_start();
    if !after.starts_with('^') {
        return None;
    }
    let caret_col = line.len() - after.len();
    let caret_len = after.bytes().take_while(|&b| b == b'^').count();
    let content = after[caret_len..].trim();
    assert!(
        !content.is_empty(),
        "annotation line `{line}` has no expected type after the carets"
    );
    Some((caret_col, caret_len, content))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Expr,
    Pattern,
    /// A binding re-keyed to just its introduced name (`x`, not `let x`) --
    /// the natural caret target. Excluded from the dump.
    BindingName,
}

pub(crate) struct TypedNode {
    pub(crate) range: TextRange,
    pub(crate) kind: NodeKind,
    pub(crate) ty: String,
}

/// Per-body-owner hir_ty inference state paired with the maps needed to
/// resolve arena ids to source ranges.
struct OwnerInference<'db> {
    body: OwnerBody,
    source_map: AstSourceMap,
    result: InferenceResult<'db>,
}

/// Runs the hir_ty engine over every body owner in `file` and renders each
/// inferred expression and binding with its source range.
/// Compiler-synthesized nodes are skipped: fixtures assert what the user
/// wrote.
pub(crate) fn collect_hir_ty_nodes(
    db: &ProjectDatabase,
    file: baml_base::SourceFile,
    fixture: &str,
) -> Vec<TypedNode> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);

    // Inference results per body owner, keyed by the owner's scope so
    // binding lookups (which start from arbitrary child scopes) can find the
    // arena owner's result.
    let mut owners: BTreeMap<u32, OwnerInference<'_>> = BTreeMap::new();
    for owner in baml_compiler2_ppir::file_body_owners(db, file) {
        let Some(scope_id) = baml_compiler2_ppir::body_scope(db, owner) else {
            continue;
        };
        let body = baml_compiler2_ppir::body(db, owner);
        if body.expr_body().is_none() {
            continue;
        }
        let Some(source_map) = baml_compiler2_ppir::body_source_map(db, owner) else {
            continue;
        };
        owners.insert(
            scope_id.file_scope_id(db).index(),
            OwnerInference {
                body,
                source_map,
                result: infer_body(db, owner).clone(),
            },
        );
    }

    let mut nodes = Vec::new();
    // Each function's inferred EFFECT, keyed at the function NAME range -
    // the caret target for `throws` pins (S12); differential against
    // TIR's callable_throws below.
    for owner_id in baml_compiler2_ppir::file_body_owners(db, file) {
        let baml_compiler2_hir::body::BodyOwnerId::Function(function) = owner_id else {
            continue;
        };
        let Some(scope_id) = baml_compiler2_ppir::body_scope(db, owner_id) else {
            continue;
        };
        let Some(owner) = owners.get(&scope_id.file_scope_id(db).index()) else {
            continue;
        };
        let name_span = baml_compiler2_ppir::item_data::function_source_map(db, function).name_span;
        if !name_span.is_empty() {
            nodes.push(TypedNode {
                range: name_span,
                kind: NodeKind::Expr,
                ty: format!(
                    "throws {}",
                    owner.result.throws.to_plain().render_canonical()
                ),
            });
        }
    }
    for owner in owners.values() {
        for (&expr_id, ty) in &owner.result.type_of_expr {
            if owner.source_map.is_synthetic_expr(expr_id) {
                continue;
            }
            nodes.push(TypedNode {
                range: owner.source_map.expr_span(expr_id),
                kind: NodeKind::Expr,
                ty: ty.to_plain().render_canonical(),
            });
        }
        let Some(body) = owner.body.expr_body() else {
            continue;
        };
        for (pat_id, _) in body.patterns.iter() {
            if owner.source_map.synthetic_patterns.contains(&pat_id) {
                continue;
            }
            if let Some(ty) = owner.result.type_of_pat.get(&pat_id) {
                nodes.push(TypedNode {
                    range: owner.source_map.pattern_span(pat_id),
                    kind: NodeKind::Pattern,
                    ty: ty.to_plain().render_canonical(),
                });
            }
        }
    }

    // Bindings again, keyed by just the introduced name, which is the natural
    // caret target. A binding lives in the scope that declares it (often a
    // Block), so resolve through the enclosing body owner's result.
    for (idx, bindings) in index.scope_bindings.iter().enumerate() {
        if bindings.bindings.is_empty() {
            continue;
        }
        let Some(owner_scope) = enclosing_owner_scope(index, FileScopeId::new(idx as u32)) else {
            continue;
        };
        let Some(owner) = owners.get(&owner_scope.index()) else {
            continue;
        };
        for binding in &bindings.bindings {
            if let Some(ty) = owner.result.type_of_pat.get(&binding.bind_pattern) {
                nodes.push(TypedNode {
                    range: binding_name_range(fixture, binding),
                    kind: NodeKind::BindingName,
                    ty: ty.to_plain().render_canonical(),
                });
            }
        }
    }

    nodes.retain(|node| !node.range.is_empty());
    nodes
}

/// The range of just the introduced identifier. A `let x` / `let x: T` Bind
/// pattern's `name_range` currently spans the whole pattern including the
/// keyword and annotation, so narrow to the first word-boundary occurrence of
/// the name; ranges that already are the bare name pass through unchanged.
fn binding_name_range(
    fixture: &str,
    binding: &baml_compiler2_hir::semantic_index::LocalBinding,
) -> TextRange {
    let name = binding.name.as_str();
    let text = &fixture[binding.name_range];
    if text == name {
        return binding.name_range;
    }
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut search_from = 0;
    while let Some(idx) = text[search_from..].find(name).map(|i| search_from + i) {
        let before_ok = idx == 0 || !is_ident(text.as_bytes()[idx - 1]);
        let after = idx + name.len();
        let after_ok = after == text.len() || !is_ident(text.as_bytes()[after]);
        if before_ok && after_ok {
            let start = u32::from(binding.name_range.start()) + idx as u32;
            return TextRange::new(
                TextSize::new(start),
                TextSize::new(start + name.len() as u32),
            );
        }
        search_from = idx + 1;
    }
    binding.name_range
}

/// Walks to the enclosing scope (self included) that belongs to a body owner
/// (function or top-level let) -- the arena owner for everything nested in
/// it, lambdas included.
fn enclosing_owner_scope(
    index: &FileSemanticIndex<'_>,
    mut fsi: FileScopeId,
) -> Option<FileScopeId> {
    loop {
        let scope = &index.scopes[fsi.index() as usize];
        match scope.kind {
            ScopeKind::Function | ScopeKind::Let => return Some(fsi),
            _ => fsi = scope.parent?,
        }
    }
}
