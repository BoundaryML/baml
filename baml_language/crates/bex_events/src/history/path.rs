use std::path::{Path, PathBuf};

use crate::{
    ids::BoundaryId,
    run::{RunTarget, StartRunContext},
};

pub const HISTORY_DIR_NAME: &str = "history";
const BAML_TOML: &str = "baml.toml";
const BAML_SRC_DIR: &str = "baml_src";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryHistoryPath {
    pub project_root: PathBuf,
    pub boundary_dir: PathBuf,
}

impl BoundaryHistoryPath {
    #[must_use]
    pub fn thread_dir(&self, thread_id: u64) -> PathBuf {
        self.boundary_dir.join(format!("thread-{thread_id}"))
    }

    #[must_use]
    pub fn stack_segment_path(&self, thread_id: u64, segment: u64) -> PathBuf {
        self.thread_dir(thread_id)
            .join(format!("stack-{segment}.bamlprof"))
    }

    #[must_use]
    pub fn value_segment_path(&self, thread_id: u64, segment: u64) -> PathBuf {
        self.thread_dir(thread_id)
            .join(format!("value-{segment}.bamlvalue"))
    }
}

#[must_use]
pub fn build_boundary_history_path(
    project_root: impl AsRef<Path>,
    start: &StartRunContext,
) -> BoundaryHistoryPath {
    let project_root = project_root.as_ref().to_path_buf();
    let history_root = project_root.join(".baml").join(HISTORY_DIR_NAME);
    let dir_name = format!(
        "{}-{}-{}",
        timestamp_slug(start.created_at_ms),
        target_slug(&start.request.target),
        start.boundary_id.to_wire_string()
    );
    BoundaryHistoryPath {
        project_root,
        boundary_dir: history_root.join(dir_name),
    }
}

#[must_use]
pub fn find_boundary_dir(search_roots: &[PathBuf], boundary_id: BoundaryId) -> Option<PathBuf> {
    let needle = boundary_id.to_wire_string();
    for root in search_roots {
        for history_root in candidate_history_roots(root) {
            let Ok(entries) = std::fs::read_dir(&history_root) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(&needle))
                {
                    return Some(path);
                }
            }
        }
    }
    None
}

#[must_use]
pub fn list_boundary_dirs(search_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root in search_roots {
        for history_root in candidate_history_roots(root) {
            let Ok(entries) = std::fs::read_dir(&history_root) else {
                continue;
            };
            dirs.extend(
                entries
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir()),
            );
        }
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

#[must_use]
pub fn resolve_project_root(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let mut nearest_baml_src_owner = None;
    for ancestor in path.ancestors() {
        if ancestor.join(BAML_TOML).is_file() {
            return ancestor.to_path_buf();
        }
        if nearest_baml_src_owner.is_none() && ancestor.join(BAML_SRC_DIR).is_dir() {
            nearest_baml_src_owner = Some(ancestor.to_path_buf());
        }
    }
    nearest_baml_src_owner.unwrap_or_else(|| path.to_path_buf())
}

fn candidate_history_roots(root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = resolve_project_root(root);
    roots.push(root.join(".baml").join(HISTORY_DIR_NAME));
    collect_nested_history_roots(&root, 0, &mut roots);
    roots.sort();
    roots.dedup();
    roots
}

fn collect_nested_history_roots(root: &Path, depth: usize, roots: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if matches!(name, ".git" | "node_modules" | "target" | ".next") {
            continue;
        }
        if path.join(BAML_TOML).is_file() || path.join(BAML_SRC_DIR).is_dir() {
            roots.push(path.join(".baml").join(HISTORY_DIR_NAME));
        }
        collect_nested_history_roots(&path, depth + 1, roots);
    }
}

fn timestamp_slug(created_at_ms: u64) -> String {
    format!("{created_at_ms}")
}

fn target_slug(target: &RunTarget) -> String {
    let raw = match target {
        RunTarget::Function { function_name } => function_name.as_str(),
        RunTarget::Test { test_name, .. } => test_name.as_str(),
        RunTarget::Preview {
            parent_function_name,
            ..
        } => parent_function_name.as_str(),
        RunTarget::Companion { function_name, .. } => function_name.as_str(),
        RunTarget::Internal { name } => name.as_str(),
    };
    let mut slug = String::with_capacity(raw.len().min(80));
    for ch in raw.chars().take(80) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            slug.push(ch);
        } else {
            slug.push('_');
        }
    }
    if slug.is_empty() {
        "boundary".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "baml-history-path-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_project_root_accepts_manifestless_baml_src_project() {
        let root = temp_dir("manifestless");
        let nested = root.join("baml_src/ns");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("main.baml"), "function Test() -> int { 1 }\n").unwrap();

        assert_eq!(resolve_project_root(&root), root);
        assert_eq!(resolve_project_root(&nested), root);
    }

    #[test]
    fn resolve_project_root_prefers_manifest_over_nearer_baml_src_marker() {
        let root = temp_dir("manifest-preferred");
        let nested = root.join("pkg/baml_src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("baml.toml"), "[package]\nname = \"root\"\n").unwrap();

        assert_eq!(resolve_project_root(&nested), root);
    }

    #[test]
    fn list_boundary_dirs_searches_manifestless_nested_projects() {
        let workspace = temp_dir("nested-manifestless");
        let project = workspace.join("demo");
        let history = project.join(".baml/history/1-Test-baml_id_1_AAAAAAAAAAAAAAAAAAAAAA");
        fs::create_dir_all(project.join("baml_src")).unwrap();
        fs::create_dir_all(&history).unwrap();

        assert_eq!(list_boundary_dirs(&[workspace]), vec![history]);
    }

    #[test]
    fn resolve_project_root_falls_back_to_input_path_without_project_marker() {
        let root = temp_dir("no-marker");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();

        assert_eq!(resolve_project_root(&child), child);
    }

    #[test]
    fn target_slug_never_returns_empty() {
        assert_eq!(
            target_slug(&RunTarget::Internal {
                name: String::new()
            }),
            "boundary"
        );
    }

    #[test]
    fn candidate_history_roots_includes_direct_marker() {
        let root = temp_dir("candidate-root");
        fs::create_dir_all(root.join("baml_src")).unwrap();

        assert!(
            candidate_history_roots(Path::new(&root))
                .contains(&root.join(".baml").join(HISTORY_DIR_NAME))
        );
    }
}
