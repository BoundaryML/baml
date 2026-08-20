//! Owner reactions to [`OwnerEvent`]s: request completions, diagnostics
//! tails, discovery and reload results.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use baml_base::SourceRoot;

use super::notifications::log_rejections;
use crate::{
    diagnostics,
    discovery::LoadedRoot,
    error::LspError,
    executor::{ReadOutcome, spawn_read},
    mutation::SourceMutation,
    snapshot::{RequestCx, TaskFailure},
    state::{DiagnosticCandidate, GlobalState, OwnerEvent},
};

impl GlobalState {
    /// React to one event from [`GlobalState::events`]. Exhaustive over
    /// [`OwnerEvent`].
    pub fn handle_event(&mut self, event: OwnerEvent) {
        match event {
            OwnerEvent::RequestDone {
                session,
                request_id,
                respond,
                outcome,
            } => {
                self.finish_read(session, &request_id);
                respond(
                    outcome
                        .map_err(LspError::from)
                        .and_then(std::convert::identity),
                );
            }
            OwnerEvent::DiagnosticsDue { root } => self.on_diagnostics_due(root),
            OwnerEvent::DiagnosticsResult { root, outcome } => {
                self.on_diagnostics_result(root, outcome);
            }
            OwnerEvent::RootsLoaded { folder, roots } => {
                self.on_roots_loaded(folder.as_deref(), roots);
            }
            OwnerEvent::FilesReloaded { files } => self.on_files_reloaded(files),
            OwnerEvent::Call(f) => f(self),
        }
    }

    /// Start a diagnostics pass for `root` unless one is already running
    /// (its completion re-arms if the root is still dirty). The pass runs on
    /// a session-free snapshot: encodings only matter at publication.
    fn on_diagnostics_due(&mut self, root: SourceRoot) {
        let Some(root_state) = self.root_state_mut(root) else {
            // Removed since the tail was armed.
            return;
        };
        if root_state.diagnostics_in_flight {
            return;
        }
        root_state.diagnostics_in_flight = true;
        let snap = self.snapshot(RequestCx::default());
        let handle = self.handle();
        spawn_read(
            self.diagnostics_executor(),
            snap,
            move |snap| diagnostics::collect_root_candidate(snap, root),
            move |outcome| handle.post(OwnerEvent::DiagnosticsResult { root, outcome }),
        );
    }

    fn on_diagnostics_result(
        &mut self,
        root: SourceRoot,
        outcome: ReadOutcome<DiagnosticCandidate>,
    ) {
        let Some(root_state) = self.root_state_mut(root) else {
            return;
        };
        root_state.diagnostics_in_flight = false;
        match outcome {
            Ok(Ok(candidate)) => diagnostics::publish_candidate(self, &candidate),
            Ok(Err(error)) => {
                tracing::error!(?root, %error, "diagnostics pass failed; retry on the next edit");
                return;
            }
            // A mutation landed: it re-armed the tail itself.
            Err(TaskFailure::Cancelled(salsa::Cancelled::PendingWrite)) => {}
            // A panicking query on another thread unwound this pass; the
            // memo was not stored, so an immediate retry would re-run the
            // panicking query from this thread and panic outright. Same
            // policy as a local panic: wait for the next edit.
            Err(TaskFailure::Cancelled(salsa::Cancelled::PropagatedPanic)) => {
                tracing::error!(
                    ?root,
                    "another thread's query panicked under the diagnostics pass; retry on the next edit"
                );
                return;
            }
            // `Local` (nothing cancels a diagnostics token today) and any
            // future variant: nothing re-armed the tail, so a blind repost
            // could spin — fail safe and wait for the next edit.
            Err(TaskFailure::Cancelled(other)) => {
                tracing::warn!(
                    ?root,
                    ?other,
                    "diagnostics pass cancelled; retry on the next edit"
                );
                return;
            }
            Err(TaskFailure::Panicked(message)) => {
                tracing::error!(?root, %message, "diagnostics pass panicked; retry on the next edit");
                return;
            }
        }
        // A tail that fired while this pass ran was skipped, and a stale
        // candidate was discarded: either way the root still owes a
        // publication and no timer is pending for it.
        if let Some(root_state) = self.root_state(root)
            && root_state.fence.is_dirty()
            && root_state.diagnostics_due.is_none()
            && !root_state.diagnostics_in_flight
        {
            self.handle().post(OwnerEvent::DiagnosticsDue { root });
        }
    }

    /// Install discovered roots. Provisional roots inside (or at) a
    /// discovered root are superseded by it; roots previously found under
    /// `folder` that discovery no longer reports are removed if no document
    /// under them is open. Files skipped because they were open are re-fed
    /// from the overlay (or the database, if closed meanwhile).
    fn on_roots_loaded(&mut self, folder: Option<&Path>, roots: Vec<LoadedRoot>) {
        let discovered: HashSet<PathBuf> =
            roots.iter().map(|root| root.spec.path.clone()).collect();
        let mut batch: Vec<SourceMutation> = Vec::new();

        let superseded: Vec<PathBuf> = self
            .provisional_roots()
            .filter(|provisional| {
                discovered
                    .iter()
                    .any(|found| provisional.starts_with(found))
            })
            .map(Path::to_path_buf)
            .collect();
        for provisional in superseded {
            if !discovered.contains(&provisional) {
                batch.push(SourceMutation::RemoveRoot {
                    path: provisional.clone(),
                });
            }
            self.unmark_provisional_root(&provisional);
        }

        if let Some(folder) = folder {
            let stale: Vec<PathBuf> = self
                .roots()
                .workspace_roots()
                .filter(|entry| entry.path.starts_with(folder))
                .filter(|entry| !discovered.contains(&entry.path))
                .filter(|entry| !self.is_provisional_root(&entry.path))
                .filter(|entry| self.open_documents_under(&entry.path).next().is_none())
                .map(|entry| entry.path.clone())
                .collect();
            batch.extend(
                stale
                    .into_iter()
                    .map(|path| SourceMutation::RemoveRoot { path }),
            );
        }

        for root in roots {
            let LoadedRoot {
                spec,
                mut files,
                unread,
            } = root;
            for path in unread {
                if self.open_document(&path).is_some() {
                    // The overlay is merged by `UpsertRoot` itself.
                    continue;
                }
                match self.file_text(&path) {
                    Some(text) => files.push((path, text)),
                    None => tracing::debug!(
                        path = %path.display(),
                        "closed before discovery finished and not in the database; the reload will add it"
                    ),
                }
            }
            batch.push(SourceMutation::UpsertRoot { spec, files });
        }

        let applied = self.apply(batch);
        log_rejections(&applied, "discovery");
        for (mutation, error) in &applied.rejected {
            if let SourceMutation::UpsertRoot { .. } = mutation {
                let params = lsp_types::ShowMessageParams {
                    typ: lsp_types::MessageType::WARNING,
                    message: error.to_string(),
                };
                match serde_json::to_value(params) {
                    Ok(value) => self.notify_all(
                        <lsp_types::notification::ShowMessage as lsp_types::notification::Notification>::METHOD,
                        &value,
                    ),
                    Err(error) => tracing::error!(%error, "showMessage params did not serialize"),
                }
            }
        }
    }

    fn on_files_reloaded(&mut self, files: Vec<(PathBuf, Option<String>)>) {
        let batch = files
            .into_iter()
            .map(|(path, text)| match text {
                Some(text) => SourceMutation::SetDisk { path, text },
                None => SourceMutation::RemoveFile { path },
            })
            .collect();
        let applied = self.apply(batch);
        // A reload can legitimately outlive its root (closed document in a
        // removed provisional root, a file created outside every project).
        for (mutation, error) in &applied.rejected {
            tracing::debug!(?mutation, %error, "reload not applied");
        }
    }
}
