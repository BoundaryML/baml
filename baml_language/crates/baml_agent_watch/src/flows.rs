//! Keeps each touched BAML project's workflows current.
//!
//! When `.baml` files change, the projects that own them are recompiled (after
//! a short settle, since an agent often writes several files in a row) and
//! their workflows are sent if they changed. The compiler lives behind
//! [`FlowProvider`] so this crate stays free of it; `baml agent watch` plugs
//! in the CLI's project loader.

use std::{
    collections::BTreeSet,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::Duration,
};

use serde_json::Value;

use crate::{
    protocol::{FlowsMessage, Workflow},
    session::Session,
    watcher::relative,
};

/// An entry function's control-flow graph, from a [`FlowProvider`].
pub struct ProjectWorkflow {
    pub name: String,
    /// The declaring file, relative to the project root.
    pub file: String,
    /// The prepared `ControlFlowGraph` as JSON.
    pub graph: Value,
}

pub trait FlowProvider: Send + Sync + 'static {
    /// The root of the BAML project that owns `file`, if any.
    fn project_root(&self, file: &Path) -> Option<PathBuf>;
    /// The workflows of the project at `root`: every function no other
    /// function calls, with its callees inlined.
    fn workflows(&self, root: &Path) -> anyhow::Result<Vec<ProjectWorkflow>>;
}

const SETTLE: Duration = Duration::from_millis(350);

/// Start the recompile thread; send it changed `.baml` files.
pub(crate) fn spawn(
    session: Arc<Session>,
    provider: Arc<dyn FlowProvider>,
) -> mpsc::Sender<Vec<PathBuf>> {
    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let _ = std::thread::Builder::new()
        .name("agent-watch-flows".into())
        .spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut files: BTreeSet<PathBuf> = first.into_iter().collect();
                while let Ok(more) = rx.recv_timeout(SETTLE) {
                    files.extend(more);
                }
                let projects: BTreeSet<PathBuf> = files
                    .iter()
                    .filter_map(|f| provider.project_root(f))
                    .collect();
                for project in projects {
                    refresh(&session, provider.as_ref(), &project);
                }
            }
        });
    tx
}

pub(crate) fn refresh(session: &Session, provider: &dyn FlowProvider, project: &Path) {
    if let Some(flows) = project_flows(session.root(), provider, project) {
        session.set_flows(flows);
    }
}

/// The `flows` message for `project`, with paths relative to `root`; `None`
/// when the project does not compile far enough to have workflows.
fn project_flows(root: &Path, provider: &dyn FlowProvider, project: &Path) -> Option<FlowsMessage> {
    // The compiler is not expected to panic on half-written sources, but an
    // agent writes a lot of half-written sources; never take the server down.
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| provider.workflows(project)));
    let workflows = match result {
        Ok(Ok(workflows)) => workflows,
        Ok(Err(err)) => {
            tracing::debug!(
                "agent watch: no workflows for {}: {err:#}",
                project.display()
            );
            return None;
        }
        Err(_) => {
            tracing::warn!(
                "agent watch: building workflows for {} panicked",
                project.display()
            );
            return None;
        }
    };
    let workflows = workflows
        .into_iter()
        .map(|w| {
            let mut graph = w.graph;
            rebase_span_paths(&mut graph, project, root);
            Workflow {
                name: w.name,
                path: relative(root, &project.join(&w.file)).unwrap_or(w.file),
                graph,
            }
        })
        .collect();
    Some(FlowsMessage {
        project: relative(root, project).unwrap_or_else(|| ".".to_string()),
        workflows,
    })
}

/// The `flows` message the server would send for the project that owns
/// `root`, as JSON (`{"project":..,"workflows":[..]}`). Used to record
/// sessions offline.
pub fn workflows_json(root: &Path, provider: &dyn FlowProvider) -> anyhow::Result<String> {
    let project = provider
        .project_root(&root.join("."))
        .ok_or_else(|| anyhow::anyhow!("no BAML project at {}", root.display()))?;
    let flows = project_flows(root, provider, &project).unwrap_or(FlowsMessage {
        project: relative(root, &project).unwrap_or_else(|| ".".to_string()),
        workflows: Vec::new(),
    });
    Ok(serde_json::to_string(&flows)?)
}

/// Rewrite every node's `sourceSpan.filePath` relative to the watched root,
/// so the page can match graph nodes to the files it has seen change.
fn rebase_span_paths(graph: &mut Value, project: &Path, root: &Path) {
    let Some(nodes) = graph.get_mut("nodes").and_then(Value::as_object_mut) else {
        return;
    };
    for node in nodes.values_mut() {
        let Some(path) = node.pointer_mut("/sourceSpan/filePath") else {
            continue;
        };
        let Some(raw) = path.as_str() else { continue };
        let abs = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            project.join(raw)
        };
        if let Some(rel) = relative(root, &abs) {
            *path = Value::String(rel);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn span_paths_become_root_relative() {
        let mut graph = json!({
            "nodes": {
                "0": { "sourceSpan": { "filePath": "/w/app/baml_src/a.baml" } },
                "1": { "sourceSpan": { "filePath": "baml_src/b.baml" } },
                "2": { "label": "no span" }
            }
        });
        rebase_span_paths(&mut graph, Path::new("/w/app"), Path::new("/w"));
        assert_eq!(
            graph["nodes"]["0"]["sourceSpan"]["filePath"],
            "app/baml_src/a.baml"
        );
        assert_eq!(
            graph["nodes"]["1"]["sourceSpan"]["filePath"],
            "app/baml_src/b.baml"
        );
    }
}
