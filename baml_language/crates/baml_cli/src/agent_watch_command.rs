use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use baml_agent_watch::{FlowProvider, ProjectWorkflow, WatchOptions};
use baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization;
use clap::Args;

use crate::{
    ExitCode,
    playground_command::{is_headless_session, resolve_playground_assets},
    project_load::{
        build_db_from_sources, find_project_root_from, resolve_project_sources_lenient,
    },
};

/// Watch a directory and see what a coding agent is doing to it.
///
/// Opens the Agent Activity page: every file the agent changes under DIR,
/// animated as it happens (anything git ignores is skipped), the BAML
/// workflows those changes touch, and, when the agent keeps a transcript as
/// Claude Code does, what it said and which commands it ran. Opens a browser
/// unless the session is headless.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Watch the current directory:
    baml agent watch

  Watch another directory on a fixed port, without opening a browser:
    baml agent watch ../my-app --port 4285 --no-open")]
pub(crate) struct AgentWatchArgs {
    /// Directory to watch. Default: the current directory.
    #[arg(value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// Listen on exactly this port (errors if unavailable).
    /// Default: the first free port from 4285.
    #[arg(long, value_name = "PORT", help_heading = "Server options")]
    pub port: Option<u16>,

    /// Do not open a browser. Opening is also skipped automatically in
    /// headless sessions (SSH, or no display on Linux).
    #[arg(long, help_heading = "Server options")]
    pub no_open: bool,

    /// Do not read the agent's transcript, which provides captions and the
    /// commands it runs.
    #[arg(long)]
    pub no_transcripts: bool,
}

impl AgentWatchArgs {
    #[expect(
        clippy::print_stdout,
        reason = "the banner is this command's primary output"
    )]
    pub fn run(&self) -> Result<ExitCode> {
        let dir = match &self.dir {
            Some(dir) => dir.clone(),
            None => std::env::current_dir().context("could not read the current directory")?,
        };
        let root = dir
            .canonicalize()
            .with_context(|| format!("cannot watch `{}`", dir.display()))?;
        let bundle_dir = resolve_playground_assets()?;
        let open = !self.no_open && !is_headless_session(|key| std::env::var_os(key).is_some());
        let banner_root = root.clone();
        baml_agent_watch::run(WatchOptions {
            root,
            port: self.port,
            bundle_dir,
            transcripts: !self.no_transcripts,
            flows: Some(Arc::new(CliFlows)),
            on_listening: Some(Box::new(move |port| {
                let url = format!("http://localhost:{port}/agent-activity");
                println!("{}", watch_banner(&url, port, &banner_root));
                if open && let Err(err) = webbrowser::open(&url) {
                    crate::reporter::print_warning(format_args!("could not open a browser: {err}"));
                }
            })),
        })?;
        Ok(ExitCode::Success)
    }
}

/// Print the workflows `baml agent watch` would send for the BAML project
/// that owns DIR, as `{ "project": ..., "workflows": [...] }` with paths
/// relative to DIR.
#[derive(Args, Clone, Debug)]
pub(crate) struct AgentWorkflowsArgs {
    #[arg(value_name = "DIR")]
    pub dir: PathBuf,
}

impl AgentWorkflowsArgs {
    #[expect(clippy::print_stdout, reason = "the JSON is this command's output")]
    pub fn run(&self) -> Result<ExitCode> {
        let root = self
            .dir
            .canonicalize()
            .with_context(|| format!("cannot read `{}`", self.dir.display()))?;
        let json = baml_agent_watch::workflows_json(&root, &CliFlows)?;
        println!("{json}");
        Ok(ExitCode::Success)
    }
}

fn watch_banner(url: &str, port: u16, root: &Path) -> String {
    format!(
        "\n  Agent activity:  {url}\n  Watching:        {}\n\n  \
         Remote machine? Forward the port, then open the URL locally:\n    \
         ssh -L {port}:localhost:{port} <user@host>\n\n  Press Ctrl-C to stop.\n",
        root.display()
    )
}

/// Workflows from the compiler: every user function that no other user
/// function calls, as the playground graphs it (callees inlined).
struct CliFlows;

impl FlowProvider for CliFlows {
    fn project_root(&self, file: &Path) -> Option<PathBuf> {
        let dir = if file.is_dir() { file } else { file.parent()? };
        find_project_root_from(Some(dir)).ok().flatten()
    }

    fn workflows(&self, root: &Path) -> Result<Vec<ProjectWorkflow>> {
        let Some(resolved) = resolve_project_sources_lenient(Some(root))? else {
            return Ok(Vec::new());
        };
        let (db, package) = build_db_from_sources(&resolved, |_| {});
        let files = db.workspace_files();
        let listing = baml_ide::list_functions_with_metadata(&db, package);
        let graphs: Vec<_> = listing
            .functions
            .iter()
            .filter(|f| f.origin == baml_ide::FunctionOrigin::UserDefined && !f.is_sub_function)
            .filter_map(|f| {
                let graph = baml_ide::ast_control_flow_graph(&db, &files, &f.name)?;
                Some((f, prepare_control_flow_graph_for_visualization(&graph)))
            })
            .collect();

        // A function some other function calls is a step of that workflow,
        // not its own. The graph only has nodes for what the playground draws
        // (LLM calls, branches, loops), so a plain call like `Double(x)`
        // leaves no node; read the calls from each function's source instead.
        let sources: std::collections::HashMap<&Path, &str> = resolved
            .files
            .iter()
            .map(|(path, text)| (path.as_path(), text.as_str()))
            .collect();
        let names: Vec<&str> = graphs.iter().map(|(f, _)| short_name(&f.name)).collect();
        let mut called: HashSet<String> = HashSet::new();
        for (f, graph) in &graphs {
            let Some(body) = function_body(graph, &sources) else {
                continue;
            };
            for name in &names {
                if *name != short_name(&f.name) && calls(body, name) {
                    called.insert((*name).to_string());
                }
            }
        }

        graphs
            .into_iter()
            .filter(|(f, _)| !called.contains(short_name(&f.name)))
            .map(|(f, graph)| {
                Ok(ProjectWorkflow {
                    name: f.name.clone(),
                    file: f.source_position.file.clone(),
                    graph: serde_json::to_value(&graph)?,
                })
            })
            .collect()
    }
}

/// The unqualified name: `ns.Extract` → `Extract`.
fn short_name(name: &str) -> &str {
    name.rsplit(['.', ':']).next().unwrap_or(name)
}

/// The source of a function's body, from its graph's root span.
fn function_body<'a>(
    graph: &baml_compiler2_visualization::control_flow::ControlFlowGraph,
    sources: &std::collections::HashMap<&Path, &'a str>,
) -> Option<&'a str> {
    let span = graph
        .nodes
        .values()
        .find(|n| n.parent_node_id.is_none())?
        .source_span
        .as_ref()?;
    let text = sources.get(Path::new(&span.file_path))?;
    let decl = text.get(span.start_offset as usize..span.end_offset as usize)?;
    decl.find('{').map(|brace| &decl[brace + 1..])
}

/// Whether `body` calls `name`: the name as a whole word, followed by `(`.
fn calls(body: &str, name: &str) -> bool {
    body.match_indices(name).any(|(at, _)| {
        let before = body[..at].chars().next_back();
        let after = body[at + name.len()..].trim_start().chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && after == Some('(')
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn workflows_are_the_functions_nothing_else_calls() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("baml.toml"),
            "[package]\nname = \"flows\"\n",
        )
        .unwrap();
        fs::create_dir(dir.path().join("baml_src")).unwrap();
        fs::write(
            dir.path().join("baml_src/main.baml"),
            "function Double(x: int) -> int {\n    x * 2\n}\n\n\
             function Quadruple(x: int) -> int {\n    Double(Double(x))\n}\n\n\
             function Alone() -> int {\n    1\n}\n",
        )
        .unwrap();
        let root = CliFlows
            .project_root(&dir.path().join("baml_src/main.baml"))
            .unwrap();
        let mut names: Vec<String> = CliFlows
            .workflows(&root)
            .unwrap()
            .into_iter()
            .map(|w| w.name)
            .collect();
        names.sort();
        assert_eq!(names, ["Alone", "Quadruple"]);
    }
}
