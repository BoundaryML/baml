//! The workspace model. It is built once and read by the checks, the fixer
//! and the graph.

use std::collections::{BTreeMap, BTreeSet};

use crate::manifest::DepTable;

#[derive(Debug)]
pub(crate) struct Workspace {
    /// Directories that hold member crates, relative to `root`.
    pub(crate) member_roots: Vec<String>,
    /// `[workspace.dependencies]`, by key.
    pub(crate) deps: BTreeMap<String, WorkspaceDep>,
    /// Sorted by name; ignored crates are absent.
    pub(crate) crates: Vec<Crate>,
}

#[derive(Debug)]
pub(crate) struct WorkspaceDep {
    /// The package the key refers to; differs from the key when renamed.
    pub(crate) package: String,
    pub(crate) features: BTreeSet<String>,
}

#[derive(Debug)]
pub(crate) struct Crate {
    pub(crate) name: String,
    pub(crate) folder: String,
    /// Relative to the workspace root, `/`-separated.
    pub(crate) dir: String,
    /// Relative to the workspace root, `/`-separated.
    pub(crate) manifest: String,
    /// Index into `Config::namespaces`.
    pub(crate) namespace: Option<usize>,
    pub(crate) tables: Vec<DepTable>,
}

impl Workspace {
    pub(crate) fn krate(&self, name: &str) -> Option<&Crate> {
        self.crates
            .binary_search_by(|krate| krate.name.as_str().cmp(name))
            .ok()
            .map(|index| &self.crates[index])
    }

    /// The package a dependency name refers to.
    pub(crate) fn package_of<'a>(&'a self, dependency: &'a str) -> &'a str {
        self.deps
            .get(dependency)
            .map_or(dependency, |dep| dep.package.as_str())
    }
}

impl Crate {
    /// The features this crate asks of `package`, across all its tables.
    pub(crate) fn features_for(&self, package: &str, workspace: &Workspace) -> BTreeSet<String> {
        self.tables
            .iter()
            .flat_map(|table| &table.entries)
            .filter(|entry| workspace.package_of(&entry.name) == package)
            .flat_map(|entry| entry.features.iter().cloned())
            .collect()
    }
}
