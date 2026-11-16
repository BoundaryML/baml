//! The crate dependency graph and the rules that thin it out (spec 9.1, 9.2).

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    config::{self, Config},
    workspace::Workspace,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    Crate(String),
    External(String),
}

impl Target {
    fn name(&self) -> &str {
        match self {
            Target::Crate(name) | Target::External(name) => name,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Graph {
    pub(crate) crates: BTreeSet<String>,
    pub(crate) edges: BTreeSet<(String, Target)>,
}

/// Every namespaced crate and its regular dependencies on other namespaced
/// crates or on the external packages the config lists.
pub(crate) fn collect(workspace: &Workspace, config: &Config) -> Graph {
    let crates: BTreeSet<String> = workspace
        .crates
        .iter()
        .filter(|krate| krate.namespace.is_some())
        .map(|krate| krate.name.clone())
        .collect();
    let edges = workspace
        .crates
        .iter()
        .filter(|krate| krate.namespace.is_some())
        .flat_map(|krate| {
            krate
                .tables
                .iter()
                .filter(|table| table.path.is_regular())
                .flat_map(|table| &table.entries)
                .map(|entry| workspace.package_of(&entry.name))
                .filter_map(|package| {
                    if package != krate.name && crates.contains(package) {
                        Some(Target::Crate(package.to_owned()))
                    } else if config.graph.external_deps.iter().any(|dep| dep == package) {
                        Some(Target::External(package.to_owned()))
                    } else {
                        None
                    }
                })
                .map(|target| (krate.name.clone(), target))
        })
        .collect();
    Graph { crates, edges }
}

/// Thins the graph for display (spec 9.2): hide test crates, drop edges that
/// another direct dependency already shows, then drop implied edges.
pub(crate) fn reduce(
    graph: &Graph,
    workspace: &Workspace,
    config: &Config,
    include_tests: bool,
) -> Graph {
    let visible = |name: &str| include_tests || !config::is_test_crate(name);
    let shown = Graph {
        crates: graph
            .crates
            .iter()
            .filter(|name| visible(name))
            .cloned()
            .collect(),
        edges: graph
            .edges
            .iter()
            .filter(|(from, to)| {
                visible(from) && (matches!(to, Target::External(_)) || visible(to.name()))
            })
            .cloned()
            .collect(),
    };

    let within = within_namespaces(&shown, workspace);
    let direct: BTreeSet<&(String, Target)> = shown
        .edges
        .iter()
        .filter(|edge| !is_transitive(edge, &shown, &within, workspace))
        .collect();

    let has_edge_to = |from: &String, name: &str| {
        direct
            .iter()
            .any(|(source, target)| source == from && target.name() == name)
    };
    let is_implied = |from: &String, to: &Target| {
        config.graph.implied_deps.iter().any(|(primary, implied)| {
            implied.iter().any(|dep| dep == to.name()) && has_edge_to(from, primary)
        })
    };
    let edges = direct
        .iter()
        .filter(|(from, to)| !is_implied(from, to))
        .map(|edge| (*edge).clone())
        .collect();

    Graph {
        crates: shown.crates,
        edges,
    }
}

/// Whether an edge only repeats what another direct dependency already shows.
fn is_transitive(
    (from, to): &(String, Target),
    graph: &Graph,
    within: &BTreeMap<&str, Vec<&str>>,
    workspace: &Workspace,
) -> bool {
    let namespace = |name: &str| workspace.krate(name).and_then(|krate| krate.namespace);
    let siblings = graph
        .edges
        .range((from.clone(), Target::Crate(String::new()))..)
        .take_while(|(source, _)| source == from)
        .filter_map(|(_, target)| match target {
            Target::Crate(name) if name != to.name() => Some(name),
            _ => None,
        });

    match to {
        Target::Crate(target) => {
            let shared = namespace(target);
            if namespace(from) != shared {
                return false;
            }
            siblings
                .filter(|sibling| namespace(sibling) == shared)
                .any(|sibling| reaches(sibling, target, within))
        }
        Target::External(package) => {
            let features = |name: &str| {
                workspace
                    .krate(name)
                    .map(|krate| krate.features_for(package, workspace))
            };
            let mine = features(from);
            siblings
                .filter(|sibling| namespace(sibling) == namespace(from))
                .filter(|sibling| graph.edges.contains(&((*sibling).clone(), to.clone())))
                .any(|sibling| features(sibling) == mine)
        }
    }
}

/// Crate-to-crate edges whose two ends share a namespace.
fn within_namespaces<'a>(
    graph: &'a Graph,
    workspace: &Workspace,
) -> BTreeMap<&'a str, Vec<&'a str>> {
    let namespace = |name: &str| workspace.krate(name).and_then(|krate| krate.namespace);
    let mut within: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (from, to) in &graph.edges {
        if let Target::Crate(to) = to {
            if namespace(from) == namespace(to) {
                within.entry(from).or_default().push(to);
            }
        }
    }
    within
}

fn reaches(from: &str, to: &str, edges: &BTreeMap<&str, Vec<&str>>) -> bool {
    let mut seen = BTreeSet::new();
    let mut pending = vec![from];
    while let Some(current) = pending.pop() {
        if current == to {
            return true;
        }
        if seen.insert(current) {
            pending.extend(edges.get(current).into_iter().flatten());
        }
    }
    false
}
