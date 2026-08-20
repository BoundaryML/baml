//! Per-root diagnostics: computed on a snapshot, published by the owner.
//!
//! [`collect_root_candidate`] runs on the executor against one `Workspace`
//! root and returns a fully owned [`DiagnosticCandidate`] (texts included).
//! [`publish_candidate`] runs on the owner: it admits the candidate through
//! the root's [`crate::state::DiagnosticsFence`], converts it once per
//! distinct session encoding, and pushes `textDocument/publishDiagnostics`
//! to every initialized session. Only `Workspace` roots are ever scheduled;
//! stdlib and dependency files are never published (a shared root's
//! mutation dirties every workspace root instead — see
//! [`crate::state::GlobalState::apply`]).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_base::{FileId, SourceRoot};
use baml_compiler_diagnostics::{Diagnostic, Severity};
use lsp_types::Url;

use crate::{
    error::LspError,
    paths,
    position_codec::{PositionCodec, PositionEncoding},
    roots::RootsView,
    snapshot::Snapshot,
    state::{CandidateFile, DiagnosticCandidate, GlobalState, ReferencedFile, SessionKey},
};

/// Check every file of `root` and bundle the results with the texts a
/// publisher needs to convert spans, including spans into other roots.
///
/// A diagnostic is published under the file its primary span points into
/// when that file belongs to the root; otherwise under the file whose check
/// produced it. Package-level diagnostics (cross-file conflicts) are folded
/// in for the root's files.
pub fn collect_root_candidate(
    snap: &Snapshot,
    root: SourceRoot,
) -> Result<DiagnosticCandidate, LspError> {
    let db = snap.db();
    let files = db.root_files(root);
    let per_file = baml_db::check_files_parallel(db, &files);

    let index_by_id: HashMap<FileId, usize> = files
        .iter()
        .enumerate()
        .map(|(index, file)| (file.file_id(db), index))
        .collect();
    let mut buckets: Vec<Vec<Diagnostic>> = files.iter().map(|_| Vec::new()).collect();
    for (checked, diagnostics) in per_file.into_iter().enumerate() {
        for diagnostic in diagnostics {
            let target = diagnostic
                .primary_span()
                .and_then(|span| index_by_id.get(&span.file_id).copied())
                .unwrap_or(checked);
            // Re-attributed diagnostics can be produced by more than one
            // file's check; the same one is published once.
            if target != checked && buckets[target].contains(&diagnostic) {
                continue;
            }
            buckets[target].push(diagnostic);
        }
    }
    for diagnostic in baml_db::collect_package_level_diagnostics(db) {
        let Some(target) = diagnostic
            .primary_span()
            .and_then(|span| index_by_id.get(&span.file_id).copied())
        else {
            continue;
        };
        if !buckets[target].contains(&diagnostic) {
            buckets[target].push(diagnostic);
        }
    }

    // Texts of files outside the root that any span points into.
    let mut referenced: HashMap<FileId, ReferencedFile> = HashMap::new();
    for diagnostic in buckets.iter().flatten() {
        let spans = diagnostic
            .primary_span()
            .into_iter()
            .chain(diagnostic.related_info.iter().map(|info| info.span));
        for span in spans {
            if index_by_id.contains_key(&span.file_id) || referenced.contains_key(&span.file_id) {
                continue;
            }
            let Some(path) = db.file_id_to_path(span.file_id) else {
                continue;
            };
            let Some(file) = db.get_file(path) else {
                continue;
            };
            referenced.insert(
                span.file_id,
                ReferencedFile {
                    file_id: span.file_id,
                    path: path.clone(),
                    text: Arc::from(file.text(db).as_str()),
                },
            );
        }
    }

    let files = files
        .iter()
        .zip(buckets)
        .map(|(file, diagnostics)| CandidateFile {
            file_id: file.file_id(db),
            path: file.path(db),
            text: Arc::from(file.text(db).as_str()),
            diagnostics,
        })
        .collect();
    Ok(DiagnosticCandidate {
        root,
        revision: snap.revision(),
        files,
        referenced: referenced.into_values().collect(),
    })
}

/// One publish-ready document in one encoding.
#[derive(Debug, Clone)]
pub struct PublishableDocument {
    pub path: PathBuf,
    pub diagnostics: Vec<lsp_types::Diagnostic>,
}

/// Convert a candidate with one encoding. Pure: no database, no owner
/// state. Every file of the root is represented (possibly with no
/// diagnostics) so stale editor markers clear.
pub fn candidate_to_publishable(
    candidate: &DiagnosticCandidate,
    encoding: PositionEncoding,
    roots: &RootsView,
) -> Vec<PublishableDocument> {
    let mut codecs: HashMap<FileId, (&Path, PositionCodec<'_>)> = candidate
        .files
        .iter()
        .map(|file| {
            (
                file.file_id,
                (
                    file.path.as_path(),
                    PositionCodec::new(&file.text, encoding),
                ),
            )
        })
        .collect();
    codecs.extend(candidate.referenced.iter().map(|file| {
        (
            file.file_id,
            (
                file.path.as_path(),
                PositionCodec::new(&file.text, encoding),
            ),
        )
    }));

    candidate
        .files
        .iter()
        .map(|file| PublishableDocument {
            path: file.path.clone(),
            diagnostics: file
                .diagnostics
                .iter()
                .map(|diagnostic| to_lsp_diagnostic(diagnostic, file.file_id, &codecs, roots))
                .collect(),
        })
        .collect()
}

fn to_lsp_diagnostic(
    diagnostic: &Diagnostic,
    document: FileId,
    codecs: &HashMap<FileId, (&Path, PositionCodec<'_>)>,
    roots: &RootsView,
) -> lsp_types::Diagnostic {
    let mut related_information: Vec<lsp_types::DiagnosticRelatedInformation> = Vec::new();
    let location = |file_id: FileId, range: text_size::TextRange| -> Option<lsp_types::Location> {
        let (path, codec) = codecs.get(&file_id)?;
        let uri = paths::uri_for_db_path(roots, path)?;
        Some(lsp_types::Location {
            uri,
            range: codec.byte_range_to_lsp(range),
        })
    };

    // The range in the published document. A diagnostic whose primary span
    // lies in another file (or that has no span) is anchored at the
    // document start, with the real location as related information.
    let range = match diagnostic.primary_span() {
        Some(span) if span.file_id == document => codecs
            .get(&document)
            .map(|(_, codec)| codec.byte_range_to_lsp(span.range))
            .unwrap_or_default(),
        Some(span) => {
            if let Some(loc) = location(span.file_id, span.range) {
                related_information.push(lsp_types::DiagnosticRelatedInformation {
                    location: loc,
                    message: diagnostic.message_with_primary_label().into_owned(),
                });
            }
            lsp_types::Range::default()
        }
        None => lsp_types::Range::default(),
    };

    related_information.extend(diagnostic.related_info.iter().filter_map(|info| {
        Some(lsp_types::DiagnosticRelatedInformation {
            location: location(info.span.file_id, info.span.range)?,
            message: info.message.clone(),
        })
    }));

    lsp_types::Diagnostic {
        range,
        severity: Some(match diagnostic.severity {
            Severity::Error => lsp_types::DiagnosticSeverity::ERROR,
            Severity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            Severity::Info => lsp_types::DiagnosticSeverity::INFORMATION,
        }),
        code: Some(lsp_types::NumberOrString::String(
            diagnostic.code().to_owned(),
        )),
        code_description: None,
        source: Some("baml".to_owned()),
        message: diagnostic.message_with_primary_label().into_owned(),
        related_information: (!related_information.is_empty()).then_some(related_information),
        tags: None,
        data: None,
    }
}

/// Publish an admitted candidate to every initialized session, each in its
/// own encoding and with the exact version of the documents it has open;
/// then clear markers for files the previous publication covered but this
/// one does not. A stale candidate (a newer mutation touched the root) is
/// discarded — its own tail is already scheduled.
///
/// Files whose text and diagnostics both match the last admitted
/// publication are skipped for the sessions that received it; a session
/// seeing this root for the first time gets every file.
pub fn publish_candidate(state: &mut GlobalState, candidate: &DiagnosticCandidate) {
    let admitted = match state.root_state_mut(candidate.root) {
        Some(root_state) => root_state
            .fence
            .admit(candidate.revision, root_state.last_mutated),
        // The root was removed while the candidate computed.
        None => false,
    };
    if !admitted {
        return;
    }

    let text_hashes: Vec<u64> = candidate
        .files
        .iter()
        .map(|file| crate::state::published_text_hash(&file.text))
        .collect();
    let (changed, already_current): (HashSet<PathBuf>, HashSet<SessionKey>) = {
        let root_state = state
            .root_state(candidate.root)
            .unwrap_or_else(|| unreachable!("admitted through this root's fence above"));
        let changed = candidate
            .files
            .iter()
            .zip(&text_hashes)
            .filter(|(file, hash)| {
                !root_state
                    .fence
                    .is_unchanged(&file.path, **hash, &file.diagnostics)
            })
            .map(|(file, _)| file.path.clone())
            .collect();
        (changed, root_state.published_sessions.clone())
    };

    let roots = Arc::clone(state.roots());
    let mut converted: HashMap<PositionEncoding, Vec<PublishableDocument>> = HashMap::new();
    let mut served: HashSet<SessionKey> = HashSet::new();
    for (key, session) in state.initialized_sessions() {
        let encoding = session.encoding.unwrap_or_default();
        let full = !already_current.contains(&key);
        let documents = converted
            .entry(encoding)
            .or_insert_with(|| candidate_to_publishable(candidate, encoding, &roots));
        for document in documents.iter() {
            if !full && !changed.contains(&document.path) {
                continue;
            }
            let Some(uri) = publication_uri(state, &document.path) else {
                continue;
            };
            let version = state
                .open_document(&document.path)
                .filter(|doc| doc.session == key)
                .and_then(|doc| doc.version);
            send_publish(state, key, uri, document.diagnostics.clone(), version);
        }
        served.insert(key);
    }

    let current: HashMap<PathBuf, crate::state::PublishedFile> = candidate
        .files
        .iter()
        .zip(text_hashes)
        .map(|(file, text_hash)| {
            (
                file.path.clone(),
                crate::state::PublishedFile {
                    text_hash,
                    diagnostics: file.diagnostics.clone(),
                },
            )
        })
        .collect();
    let vanished = match state.root_state_mut(candidate.root) {
        Some(root_state) => {
            root_state.published_sessions = served;
            root_state.fence.record_publication(current)
        }
        None => Vec::new(),
    };
    publish_cleared(state, &vanished);
}

/// One empty publication per path to every initialized session, so markers
/// for files that left the database (removed root, deleted file) clear.
pub fn publish_cleared(state: &GlobalState, paths: &[PathBuf]) {
    for path in paths {
        let Some(uri) = publication_uri(state, path) else {
            continue;
        };
        for (key, _) in state.initialized_sessions() {
            send_publish(state, key, uri.clone(), Vec::new(), None);
        }
    }
}

/// The URI a publication for `path` must carry: the client's own spelling
/// while the document is open (editors match publications to buffers by
/// exact URI), the canonical presentation otherwise.
fn publication_uri(state: &GlobalState, path: &Path) -> Option<Url> {
    match state.open_document(path) {
        Some(doc) => Some(doc.uri.clone()),
        None => paths::uri_for_db_path(state.roots(), path),
    }
}

fn send_publish(
    state: &GlobalState,
    session: SessionKey,
    uri: Url,
    diagnostics: Vec<lsp_types::Diagnostic>,
    version: Option<i32>,
) {
    let Ok(session_state) = state.session(session) else {
        return;
    };
    let params = lsp_types::PublishDiagnosticsParams::new(uri, diagnostics, version);
    match serde_json::to_value(params) {
        Ok(value) => {
            if let Err(error) = session_state.sender.send_notification(
                <lsp_types::notification::PublishDiagnostics as lsp_types::notification::Notification>::METHOD,
                value,
            ) {
                tracing::warn!(?session, %error, "publishDiagnostics not delivered");
            }
        }
        Err(error) => tracing::error!(%error, "publishDiagnostics params did not serialize"),
    }
}
