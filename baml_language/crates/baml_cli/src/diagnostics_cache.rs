//! Typed (de)hydration for the per-file diagnostics cache.
//!
//! The manifest stores each file's diagnostics as an opaque borsh blob
//! (`bex_cache` never interprets it — the SeededStdlibInterface pattern). This
//! module owns the typed form: a `CachedDiagnostic` mirror keyed by
//! project-root-relative path so a stored diagnostic survives a `FileId`
//! reassignment across compiles, and the projections between it and the live
//! [`Diagnostic`].
//!
//! Soundness rests on two rules:
//! - A diagnostic whose primary span (or any span) can't be mapped to a tracked
//!   user file — e.g. it points into a `<builtin>/` stub — is **not** cacheable;
//!   its owner file is poisoned so the next compile re-checks it rather than
//!   serving an incomplete set.
//! - Any decode / rehydrate failure degrades to a re-check; the cache never
//!   serves a partial or stale set.
//!
//! The same [`CachedDiagnostic`] machinery also backs the **stdlib (builtin)
//! diagnostics** blob ([`serialize_builtin_diagnostics`] /
//! [`rehydrate_builtin_blob`]). That blob is the mirror image of the per-file
//! case: its spans are keyed by the stable `<builtin>/...` virtual path (a
//! compile-build constant) rather than a project-root-relative path, and a span
//! that points at a *user* file — not a builtin — makes it uncacheable. The
//! shared span-mapping core ([`dehydrate_with`] / [`rehydrate_with`]) keeps the
//! two keyings in one place.

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use baml_db::{
    FileId, ProjectDatabase, SourceFile, Span,
    baml_compiler_diagnostics::{
        Diagnostic, DiagnosticId, DiagnosticMessageHighlight, DiagnosticPhase, Severity,
        diagnostic::{Annotation, RelatedInfo},
    },
    baml_compiler2_hir,
};
use text_size::{TextRange, TextSize};

/// A source span keyed by project-root-relative path + byte range, so it is
/// stable across the per-process `FileId` reassignment. Each span stores its
/// own path: a cross-file secondary annotation carries a different file.
#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedSpan {
    rel_path: String,
    start: u32,
    end: u32,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedAnnotation {
    span: CachedSpan,
    message: Option<String>,
    message_highlights: Vec<DiagnosticMessageHighlight>,
    is_primary: bool,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedRelatedInfo {
    span: CachedSpan,
    message: String,
    message_highlights: Vec<DiagnosticMessageHighlight>,
    file_path: Option<String>,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct CachedDiagnostic {
    id: DiagnosticId,
    severity: Severity,
    phase: DiagnosticPhase,
    message: String,
    message_highlights: Vec<DiagnosticMessageHighlight>,
    annotations: Vec<CachedAnnotation>,
    related_info: Vec<CachedRelatedInfo>,
}

/// The manifest blob is `borsh(Option<Vec<CachedDiagnostic>>)`: `None` is a
/// poison marker (a diagnostic pointed at a builtin / untracked file, so the
/// set can't be faithfully cached) that forces the owner file to be re-checked.
type CachedFileBlob = Option<Vec<CachedDiagnostic>>;

/// Project-root-relative path for a span's file, or `None` for a builtin /
/// untracked file (which makes the whole diagnostic uncacheable).
fn rel_path_of(db: &ProjectDatabase, root: &Path, file_id: FileId) -> Option<String> {
    let path = db.file_id_to_path(file_id)?;
    if path.to_string_lossy().starts_with("<builtin>/") {
        return None;
    }
    Some(bex_cache::rel_path(root, path))
}

/// The file that owns a diagnostic for per-file caching: its primary span's
/// file, falling back to the first annotation's file (matching the renderer's
/// report anchor). `None` when neither maps to a tracked user file.
fn owner_rel(db: &ProjectDatabase, root: &Path, diag: &Diagnostic) -> Option<String> {
    let span = diag
        .primary_span()
        .or_else(|| diag.annotations.first().map(|a| a.span))?;
    rel_path_of(db, root, span.file_id)
}

fn dehydrate_span(db: &ProjectDatabase, root: &Path, span: Span) -> Option<CachedSpan> {
    Some(CachedSpan {
        rel_path: rel_path_of(db, root, span.file_id)?,
        start: span.range.start().into(),
        end: span.range.end().into(),
    })
}

/// Project a live diagnostic into its cache form, mapping each span through
/// `map_span`. `None` (short-circuited) if any span can't be mapped — the whole
/// diagnostic is then uncacheable. The per-file (rel-path) and builtin
/// (`<builtin>/...` path) keyings differ only in `map_span`.
fn dehydrate_with(
    diag: &Diagnostic,
    map_span: impl Fn(Span) -> Option<CachedSpan>,
) -> Option<CachedDiagnostic> {
    let mut annotations = Vec::with_capacity(diag.annotations.len());
    for a in &diag.annotations {
        annotations.push(CachedAnnotation {
            span: map_span(a.span)?,
            message: a.message.clone(),
            message_highlights: a.message_highlights.clone(),
            is_primary: a.is_primary,
        });
    }
    let mut related_info = Vec::with_capacity(diag.related_info.len());
    for r in &diag.related_info {
        related_info.push(CachedRelatedInfo {
            span: map_span(r.span)?,
            message: r.message.clone(),
            message_highlights: r.message_highlights.clone(),
            file_path: r.file_path.clone(),
        });
    }
    Some(CachedDiagnostic {
        id: diag.id,
        severity: diag.severity,
        phase: diag.phase,
        message: diag.message.clone(),
        message_highlights: diag.message_highlights.clone(),
        annotations,
        related_info,
    })
}

/// Rebuild a live diagnostic with current-process `FileId`s, mapping each cached
/// span through `map_span`. `None` if any span no longer maps to a tracked file
/// or is out of range — the caller degrades to a re-check.
fn rehydrate_with(
    cached: &CachedDiagnostic,
    map_span: impl Fn(&CachedSpan) -> Option<Span>,
) -> Option<Diagnostic> {
    let mut annotations = Vec::with_capacity(cached.annotations.len());
    for a in &cached.annotations {
        annotations.push(Annotation {
            span: map_span(&a.span)?,
            message: a.message.clone(),
            message_highlights: a.message_highlights.clone(),
            is_primary: a.is_primary,
        });
    }
    let mut related_info = Vec::with_capacity(cached.related_info.len());
    for r in &cached.related_info {
        related_info.push(RelatedInfo {
            span: map_span(&r.span)?,
            message: r.message.clone(),
            message_highlights: r.message_highlights.clone(),
            file_path: r.file_path.clone(),
        });
    }
    Some(Diagnostic {
        id: cached.id,
        severity: cached.severity,
        message: cached.message.clone(),
        message_highlights: cached.message_highlights.clone(),
        annotations,
        related_info,
        phase: cached.phase,
    })
}

/// Project a live diagnostic into its rel-path-keyed cache form. `None` if any
/// span can't be mapped to a tracked user file — the whole diagnostic is then
/// uncacheable (its owner file is poisoned by the caller).
fn dehydrate(db: &ProjectDatabase, root: &Path, diag: &Diagnostic) -> Option<CachedDiagnostic> {
    dehydrate_with(diag, |span| dehydrate_span(db, root, span))
}

fn rehydrate_span(db: &ProjectDatabase, root: &Path, span: &CachedSpan) -> Option<Span> {
    let full = root.join(&span.rel_path);
    let file_id = db.path_to_file_id(&full)?;
    // Guard against an out-of-range span (the file shrank): the renderer would
    // otherwise index past the source. Degrade to a re-check instead.
    let text_len = db.get_file(&full).map(|sf| sf.text(db).len() as u32)?;
    if span.end > text_len || span.start > span.end {
        return None;
    }
    Some(Span {
        file_id,
        range: TextRange::new(TextSize::new(span.start), TextSize::new(span.end)),
    })
}

/// Rebuild a live diagnostic with current-process `FileId`s. `None` if any span
/// no longer maps to a tracked file or is out of range — the caller degrades to
/// a re-check.
fn rehydrate(db: &ProjectDatabase, root: &Path, cached: &CachedDiagnostic) -> Option<Diagnostic> {
    rehydrate_with(cached, |span| rehydrate_span(db, root, span))
}

/// Serialize a diagnostic set into the opaque blob, mapping every span through
/// `map_span`. Returns a poison blob (`None` inside) if any diagnostic has an
/// unmappable span, so the owner is conservatively re-checked next compile
/// rather than served an incomplete set. The per-file (rel-path) and builtin
/// (`<builtin>/...` path) keyings differ only in `map_span`.
fn serialize_blob_with(
    diags: &[&Diagnostic],
    map_span: impl Fn(Span) -> Option<CachedSpan>,
) -> Vec<u8> {
    let mut cached = Vec::with_capacity(diags.len());
    for d in diags {
        match dehydrate_with(d, &map_span) {
            Some(c) => cached.push(c),
            None => return borsh::to_vec(&CachedFileBlob::None).unwrap_or_default(),
        }
    }
    borsh::to_vec(&CachedFileBlob::Some(cached)).unwrap_or_default()
}

/// Serialize one file's diagnostics into the opaque manifest blob (rel-path
/// keyed). Poison (re-check) if any span can't be mapped to a tracked user file.
fn serialize_file_blob(db: &ProjectDatabase, root: &Path, diags: &[&Diagnostic]) -> Vec<u8> {
    serialize_blob_with(diags, |span| dehydrate_span(db, root, span))
}

/// The blob for a checked file with no diagnostics (distinct from a poison
/// blob, which forces a re-check).
pub(crate) fn empty_blob() -> Vec<u8> {
    borsh::to_vec(&CachedFileBlob::Some(Vec::<CachedDiagnostic>::new())).unwrap_or_default()
}

/// Test helper: a valid blob fabricating one error at bytes 0..1 of `rel_path`.
/// It rehydrates cleanly (the span is in range for any non-empty file), so
/// comparing it against an honest check of an error-free file yields a genuine
/// non-empty-vs-empty mismatch — used to prove the sampled-verify tripwire
/// fires on a stale diagnostics blob.
#[cfg(test)]
pub(crate) fn one_fake_diagnostic_blob(rel_path: &str) -> Vec<u8> {
    let cached = vec![CachedDiagnostic {
        id: DiagnosticId::TypeMismatch,
        severity: Severity::Error,
        phase: DiagnosticPhase::Type,
        message: "sampled-verify poison".to_string(),
        message_highlights: Vec::new(),
        annotations: vec![CachedAnnotation {
            span: CachedSpan {
                rel_path: rel_path.to_string(),
                start: 0,
                end: 1,
            },
            message: None,
            message_highlights: Vec::new(),
            is_primary: true,
        }],
        related_info: Vec::new(),
    }];
    borsh::to_vec(&CachedFileBlob::Some(cached)).unwrap_or_default()
}

/// Decode an opaque blob and rehydrate each diagnostic, mapping every cached
/// span through `map_span`. `None` — a poison marker, an undecodable blob, or an
/// unmappable / out-of-range span — means "re-check"; the cache never serves a
/// partial or stale set.
fn rehydrate_blob_with(
    blob: &[u8],
    map_span: impl Fn(&CachedSpan) -> Option<Span>,
) -> Option<Vec<Diagnostic>> {
    let cached: CachedFileBlob = borsh::from_slice(blob).ok()?;
    let cached = cached?;
    let mut out = Vec::with_capacity(cached.len());
    for c in &cached {
        out.push(rehydrate_with(c, &map_span)?);
    }
    Some(out)
}

/// Rehydrate an opaque manifest blob (rel-path keyed) into live diagnostics.
pub(crate) fn rehydrate_file_blob(
    db: &ProjectDatabase,
    root: &Path,
    blob: &[u8],
) -> Option<Vec<Diagnostic>> {
    rehydrate_blob_with(blob, |span| rehydrate_span(db, root, span))
}

/// Group freshly-checked diagnostics by owner file and serialize each group to
/// its manifest blob. Only `check_file` output belongs here — the package-level
/// set is never cached. A diagnostic with no tracked owner file is dropped
/// (it renders at a sentinel span and is not attributable to a file).
pub(crate) fn fresh_blobs_by_file(
    db: &ProjectDatabase,
    root: &Path,
    fresh: &[Diagnostic],
) -> BTreeMap<String, Vec<u8>> {
    let mut groups: BTreeMap<String, Vec<&Diagnostic>> = BTreeMap::new();
    for d in fresh {
        if let Some(rel) = owner_rel(db, root, d) {
            groups.entry(rel).or_default().push(d);
        }
    }
    groups
        .into_iter()
        .map(|(rel, diags)| (rel, serialize_file_blob(db, root, &diags)))
        .collect()
}

// ── Stdlib (builtin) diagnostics blob ───────────────────────────────────────
//
// The builtin diagnostic set is a compile-build constant (no user file
// contributes to a stdlib package) and, for a valid stdlib, empty. It is cached
// once per toolchain under `stdlib_diagnostics_key`, so a warm dirty compile
// serves it instead of re-checking every builtin scope. Spans are keyed by the
// stable `<builtin>/...` virtual path (the mirror of the user-file rel-path
// keying), reusing the same `CachedDiagnostic` wire format.

/// The stable `<builtin>/...` virtual path for a builtin file id, or `None` for
/// a user / untracked file. A cached builtin diagnostic must reference only
/// builtin files (a user span would make the set project-dependent — decline).
fn builtin_path_of(db: &ProjectDatabase, file_id: FileId) -> Option<String> {
    let path = db.file_id_to_path(file_id)?;
    let s = path.to_string_lossy();
    s.starts_with("<builtin>/").then(|| s.into_owned())
}

fn dehydrate_builtin_span(db: &ProjectDatabase, span: Span) -> Option<CachedSpan> {
    Some(CachedSpan {
        rel_path: builtin_path_of(db, span.file_id)?,
        start: span.range.start().into(),
        end: span.range.end().into(),
    })
}

/// Live builtin files keyed by their `<builtin>/...` virtual path — the lookup
/// table for rehydrating cached builtin spans onto current-process `FileId`s.
/// Builtins live in `compiler2_file_map`, not `file_map`, so `path_to_file_id`
/// (which consults only `file_map`) never resolves them; this map is the seam.
fn builtin_files_by_path(db: &ProjectDatabase) -> HashMap<String, SourceFile> {
    baml_compiler2_hir::compiler2_all_files(db)
        .into_iter()
        .filter_map(|sf| {
            let p = sf.path(db).to_string_lossy().into_owned();
            p.starts_with("<builtin>/").then_some((p, sf))
        })
        .collect()
}

fn rehydrate_builtin_span(
    db: &ProjectDatabase,
    builtins: &HashMap<String, SourceFile>,
    span: &CachedSpan,
) -> Option<Span> {
    let sf = builtins.get(&span.rel_path)?;
    // Out-of-range guard (a builtin shrank across builds — the fingerprint would
    // normally have changed, but degrade rather than index past the source).
    let text_len = sf.text(db).len() as u32;
    if span.end > text_len || span.start > span.end {
        return None;
    }
    Some(Span {
        file_id: sf.file_id(db),
        range: TextRange::new(TextSize::new(span.start), TextSize::new(span.end)),
    })
}

/// Run `check_file` over every builtin (stdlib) file and return the honest
/// diagnostics. Called only off the warm serve path — the cold/miss store and
/// the `BAML_CACHE_VERIFY` oracle. On a database that already checked the
/// builtins (the same compile's honest pass) every scope is Salsa-memoized, so
/// this re-walk pulls no fresh inference.
pub(crate) fn collect_builtin_diagnostics(db: &ProjectDatabase) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for sf in baml_compiler2_hir::compiler2_all_files(db) {
        if sf.path(db).to_string_lossy().starts_with("<builtin>/") {
            out.extend(db.check_file(sf));
        }
    }
    out
}

/// Serialize a set of builtin-owned diagnostics into the opaque
/// stdlib-diagnostics blob (`borsh(Option<Vec<CachedDiagnostic>>)`, spans keyed
/// by `<builtin>/...` path). A poison blob (`None` inside) if any span is not a
/// builtin file, so the warm path degrades to the honest builtin check rather
/// than serving an incomplete set. (Split from
/// [`serialize_builtin_diagnostics`] so the verify oracle's negative test can
/// build a stale, non-empty blob from fabricated builtin diagnostics.)
pub(crate) fn serialize_builtin_blob(db: &ProjectDatabase, diags: &[&Diagnostic]) -> Vec<u8> {
    serialize_blob_with(diags, |span| dehydrate_builtin_span(db, span))
}

/// Run the honest builtin check and serialize it into the stdlib-diagnostics
/// blob. Expected empty for a valid stdlib, but caches whatever the honest
/// check produces.
pub(crate) fn serialize_builtin_diagnostics(db: &ProjectDatabase) -> Vec<u8> {
    let diags = collect_builtin_diagnostics(db);
    serialize_builtin_blob(db, &diags.iter().collect::<Vec<_>>())
}

/// Rehydrate the stdlib-diagnostics blob onto current-process builtin
/// `FileId`s. `None` — a poison marker, an undecodable blob, or an unmappable /
/// out-of-range span — means "check the builtins honestly"; the cache never
/// serves a partial or stale set.
pub(crate) fn rehydrate_builtin_blob(db: &ProjectDatabase, blob: &[u8]) -> Option<Vec<Diagnostic>> {
    let builtins = builtin_files_by_path(db);
    rehydrate_blob_with(blob, |span| rehydrate_builtin_span(db, &builtins, span))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use baml_db::baml_compiler_diagnostics::render;
    use text_size::TextRange;

    use super::*;

    fn span_in(db: &ProjectDatabase, path: &Path, start: u32, end: u32) -> Span {
        let file_id = db.path_to_file_id(path).expect("file present");
        Span {
            file_id,
            range: TextRange::new(TextSize::new(start), TextSize::new(end)),
        }
    }

    fn build_db(files: &[(&str, &str)]) -> (ProjectDatabase, std::path::PathBuf) {
        let root = std::path::PathBuf::from("/diag-cache-test");
        let (mut db, workspace) = crate::project_load::workspace_db(&root);
        for (name, content) in files {
            db.add_or_update_file_in(workspace, &root.join(name), content);
        }
        (db, root)
    }

    #[test]
    fn round_trip_with_cross_file_annotation_and_related_info() {
        let (db, root) = build_db(&[
            ("a.baml", "function a() -> int {\n  1\n}\n"),
            ("b.baml", "function b() -> int {\n  2\n}\n"),
        ]);
        let a = root.join("a.baml");
        let b = root.join("b.baml");

        // Primary in a.baml, a secondary annotation and related_info in b.baml.
        let diag = Diagnostic::error(DiagnosticId::DuplicateName, "duplicate `x`")
            .with_primary_span(span_in(&db, &a, 0, 8))
            .with_secondary(span_in(&db, &b, 9, 17), "also here")
            .with_related(span_in(&db, &b, 0, 4), "first here");

        let cached = dehydrate(&db, &root, &diag).expect("dehydrates");
        let bytes = borsh::to_vec(&CachedFileBlob::Some(vec![cached])).unwrap();
        let restored = rehydrate_file_blob(&db, &root, &bytes).expect("rehydrates");
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0], diag, "round-trip preserves the diagnostic");

        // And it renders byte-identically to the original.
        let mut sources = std::collections::HashMap::new();
        let mut paths = std::collections::HashMap::new();
        for p in [&a, &b] {
            let sf = db.get_file(p).unwrap();
            sources.insert(sf.file_id(&db), sf.text(&db).to_string());
            paths.insert(sf.file_id(&db), p.clone());
        }
        let cfg = render::RenderConfig::test();
        assert_eq!(
            render::render_diagnostics(&[diag], &sources, &paths, &cfg),
            render::render_diagnostics(&restored, &sources, &paths, &cfg),
            "cached diagnostic renders identically"
        );
    }

    #[test]
    fn dehydrate_declines_builtin_span() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        // A span pointing at a builtin file id can't be mapped to a rel_path.
        let builtin_file = baml_db::baml_compiler2_hir::compiler2_all_files(&db)
            .into_iter()
            .find(|sf| sf.path(&db).to_string_lossy().starts_with("<builtin>/"))
            .expect("a builtin file exists");
        let builtin_span = Span {
            file_id: builtin_file.file_id(&db),
            range: TextRange::new(TextSize::new(0), TextSize::new(3)),
        };
        let diag =
            Diagnostic::error(DiagnosticId::TypeMismatch, "bad").with_primary_span(builtin_span);
        assert!(
            dehydrate(&db, &root, &diag).is_none(),
            "a builtin-pointing diagnostic is uncacheable"
        );
        // And serializing a group containing it yields the poison blob, which
        // rehydrates to `None` (re-check).
        let blob = serialize_file_blob(&db, &root, &[&diag]);
        assert!(
            rehydrate_file_blob(&db, &root, &blob).is_none(),
            "poison degrades"
        );
    }

    #[test]
    fn rehydrate_degrades_on_missing_path_and_out_of_range() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        let a = root.join("a.baml");

        // A cached diagnostic naming a path absent from this db → None.
        let ghost = CachedDiagnostic {
            id: DiagnosticId::TypeMismatch,
            severity: Severity::Error,
            phase: DiagnosticPhase::Type,
            message: "x".to_string(),
            message_highlights: Vec::new(),
            annotations: vec![CachedAnnotation {
                span: CachedSpan {
                    rel_path: "gone.baml".to_string(),
                    start: 0,
                    end: 1,
                },
                message: None,
                message_highlights: Vec::new(),
                is_primary: true,
            }],
            related_info: Vec::new(),
        };
        let blob = borsh::to_vec(&CachedFileBlob::Some(vec![ghost])).unwrap();
        assert!(
            rehydrate_file_blob(&db, &root, &blob).is_none(),
            "missing path degrades"
        );

        // An out-of-range span (past the current file length) → None.
        let text_len = db.get_file(&a).unwrap().text(&db).len() as u32;
        let oob = CachedDiagnostic {
            id: DiagnosticId::TypeMismatch,
            severity: Severity::Error,
            phase: DiagnosticPhase::Type,
            message: "x".to_string(),
            message_highlights: Vec::new(),
            annotations: vec![CachedAnnotation {
                span: CachedSpan {
                    rel_path: "a.baml".to_string(),
                    start: 0,
                    end: text_len + 100,
                },
                message: None,
                message_highlights: Vec::new(),
                is_primary: true,
            }],
            related_info: Vec::new(),
        };
        let blob = borsh::to_vec(&CachedFileBlob::Some(vec![oob])).unwrap();
        assert!(
            rehydrate_file_blob(&db, &root, &blob).is_none(),
            "out-of-range span degrades"
        );
    }

    #[test]
    fn undecodable_blob_degrades() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        assert!(
            rehydrate_file_blob(&db, &root, &[]).is_none(),
            "an empty/legacy blob is undecodable -> re-check"
        );
        assert!(
            rehydrate_file_blob(&db, &root, &empty_blob())
                .unwrap()
                .is_empty(),
            "an explicit empty blob rehydrates to no diagnostics"
        );
    }

    #[test]
    fn builtin_blob_round_trips_a_fabricated_builtin_diagnostic() {
        let (db, _root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        // A synthetic diagnostic anchored in a real builtin file must survive
        // the `<builtin>/...`-path-keyed round-trip onto current-process FileIds.
        let builtin = baml_compiler2_hir::compiler2_all_files(&db)
            .into_iter()
            .find(|sf| sf.path(&db).to_string_lossy().starts_with("<builtin>/"))
            .expect("a builtin file exists");
        let file_id = builtin.file_id(&db);
        let diag = Diagnostic::error(DiagnosticId::TypeMismatch, "synthetic builtin diag")
            .with_primary_span(Span {
                file_id,
                range: TextRange::new(TextSize::new(0), TextSize::new(3)),
            });

        let cached = dehydrate_with(&diag, |span| dehydrate_builtin_span(&db, span))
            .expect("builtin span dehydrates");
        let blob = borsh::to_vec(&CachedFileBlob::Some(vec![cached])).unwrap();
        let restored = rehydrate_builtin_blob(&db, &blob).expect("rehydrates");
        assert_eq!(restored.len(), 1);
        assert_eq!(
            restored[0], diag,
            "round-trip preserves the builtin diagnostic"
        );
    }

    #[test]
    fn builtin_blob_declines_user_span_and_degrades() {
        let (db, root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        let a = root.join("a.baml");
        // A user-file span cannot belong to the build-constant builtin set: the
        // whole diagnostic is uncacheable as a builtin (mirror of the per-file
        // path declining a `<builtin>/` span).
        let user_diag = Diagnostic::error(DiagnosticId::TypeMismatch, "bad")
            .with_primary_span(span_in(&db, &a, 0, 3));
        assert!(
            dehydrate_with(&user_diag, |span| dehydrate_builtin_span(&db, span)).is_none(),
            "a user-file span is not cacheable as a builtin diagnostic"
        );
        // Undecodable blob → honest check.
        assert!(
            rehydrate_builtin_blob(&db, &[]).is_none(),
            "an empty/garbage blob degrades to the honest builtin check"
        );
        // An explicit empty set (the valid-stdlib case) rehydrates to nothing.
        let empty = borsh::to_vec(&CachedFileBlob::Some(Vec::<CachedDiagnostic>::new())).unwrap();
        assert!(
            rehydrate_builtin_blob(&db, &empty).unwrap().is_empty(),
            "an explicit empty builtin blob rehydrates to no diagnostics"
        );
    }

    #[test]
    fn serialize_builtin_diagnostics_round_trips_the_live_set() {
        // Whatever the honest builtin check produces (empty for a valid stdlib)
        // must round-trip through the blob unchanged and in the same order, so a
        // warm serve reproduces the honest builtin contribution exactly.
        let (db, _root) = build_db(&[("a.baml", "function a() -> int {\n  1\n}\n")]);
        let honest = collect_builtin_diagnostics(&db);
        let blob = serialize_builtin_diagnostics(&db);
        let restored = rehydrate_builtin_blob(&db, &blob).expect("live builtin blob rehydrates");
        assert_eq!(
            restored, honest,
            "the cached builtin set equals the honest builtin set"
        );
    }
}
