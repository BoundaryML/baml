//! Validation (spec section 7): a pure function from the workspace and the
//! config to a sorted list of diagnostics.

use std::{collections::BTreeSet, fmt::Write};

use crate::{
    config::{self, Config, NamingError},
    manifest::DepEntry,
    workspace::{Crate, Workspace},
};

/// Declared in report order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RuleId {
    NestedCrate,
    NameMismatch,
    Naming,
    OrphanTestCrate,
    NonWorkspaceDep,
    RestrictedDep,
    UnsortedDeps,
    UnusedWorkspaceDep,
}

impl RuleId {
    fn id(self) -> &'static str {
        match self {
            RuleId::NestedCrate => "nested-crate",
            RuleId::NameMismatch => "name-mismatch",
            RuleId::Naming => "naming",
            RuleId::OrphanTestCrate => "orphan-test-crate",
            RuleId::NonWorkspaceDep => "non-workspace-dep",
            RuleId::RestrictedDep => "restricted-dep",
            RuleId::UnsortedDeps => "unsorted-deps",
            RuleId::UnusedWorkspaceDep => "unused-workspace-dep",
        }
    }
}

/// Field order is the report order.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Diagnostic {
    krate: String,
    rule: RuleId,
    table: Option<String>,
    dependency: Option<String>,
    manifest: String,
    message: String,
}

pub(crate) fn check(workspace: &Workspace, config: &Config) -> Vec<Diagnostic> {
    let names: BTreeSet<&str> = workspace.crates.iter().map(|k| k.name.as_str()).collect();
    let mut diagnostics: Vec<Diagnostic> = workspace
        .crates
        .iter()
        .flat_map(|krate| {
            let at = |rule, table: Option<String>, dependency: Option<&str>, message| Diagnostic {
                krate: krate.name.clone(),
                rule,
                table,
                dependency: dependency.map(str::to_owned),
                manifest: krate.manifest.clone(),
                message,
            };
            let structure = [
                nested_crate(krate, &workspace.member_roots).map(|m| (RuleId::NestedCrate, m)),
                name_mismatch(krate, config).map(|m| (RuleId::NameMismatch, m)),
                naming(krate, config).map(|m| (RuleId::Naming, m)),
                orphan_test_crate(krate, &names, config).map(|m| (RuleId::OrphanTestCrate, m)),
            ]
            .into_iter()
            .flatten()
            .map(|(rule, message)| at(rule, None, None, message));

            let dependencies = krate.tables.iter().flat_map(|table| {
                let name = table.path.name();
                let located = |rule, dependency: &str, message: String| {
                    at(rule, Some(name.clone()), Some(dependency), format!("[{name}] {message}"))
                };
                let not_workspace = table
                    .entries
                    .iter()
                    .filter(|entry| !entry.workspace)
                    .map(|entry| {
                        located(
                            RuleId::NonWorkspaceDep,
                            &entry.name,
                            format!("`{}` must use `workspace = true`", entry.name),
                        )
                    });
                let restricted = table.entries.iter().flat_map(|entry| {
                    restrictions(krate, &entry.name, table.path.is_regular(), config)
                        .into_iter()
                        .map(|reason| {
                            located(
                                RuleId::RestrictedDep,
                                &entry.name,
                                format!("`{}` is not allowed here{reason}", entry.name),
                            )
                        })
                });
                let unsorted = (!table.entries.is_sorted_by_key(DepEntry::order)).then(|| {
                    at(
                        RuleId::UnsortedDeps,
                        Some(name.clone()),
                        None,
                        format!(
                            "[{name}] is not sorted: internal dependencies first, then external, each alphabetical"
                        ),
                    )
                });
                not_workspace
                    .chain(restricted)
                    .chain(unsorted)
                    .collect::<Vec<_>>()
            });

            structure.chain(dependencies).collect::<Vec<_>>()
        })
        .chain(unused_workspace_deps(workspace, &names))
        .collect();
    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}

/// One line per diagnostic, then a summary line (spec 7.3).
pub(crate) fn report(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics {
        let _ = writeln!(
            out,
            "error[{}]: {}: {}: {}",
            diagnostic.rule.id(),
            diagnostic.krate,
            diagnostic.manifest,
            diagnostic.message
        );
    }
    if diagnostics.is_empty() {
        out.push_str("ok\n");
    } else {
        let _ = writeln!(out, "{} error(s)", diagnostics.len());
    }
    out
}

fn nested_crate(krate: &Crate, member_roots: &[String]) -> Option<String> {
    let inside = |root: &str| -> Option<&str> {
        if root.is_empty() {
            return Some(krate.dir.as_str());
        }
        let rest = krate.dir.strip_prefix(root)?;
        rest.is_empty()
            .then_some(rest)
            .or_else(|| rest.strip_prefix('/'))
    };
    let (root, below) = member_roots
        .iter()
        .filter_map(|root| Some((root, inside(root)?)))
        .max_by_key(|(root, _)| root.len())?;
    (below.split('/').count() > 1).then(|| {
        let root = if root.is_empty() { "." } else { root };
        format!("`{}` must be directly under `{root}`", krate.dir)
    })
}

/// The workspace root is exempt: its folder is whatever the checkout is called.
fn name_mismatch(krate: &Crate, config: &Config) -> Option<String> {
    (!krate.dir.is_empty()
        && krate.name != krate.folder
        && !config.has_name_exception(&krate.folder, &krate.name))
    .then(|| {
        format!(
            "package name `{}` does not match folder `{}`",
            krate.name, krate.folder
        )
    })
}

fn naming(krate: &Crate, config: &Config) -> Option<String> {
    if config.has_name_exception(&krate.folder, &krate.name) {
        return None;
    }
    let error = config.check_name(&krate.name).err()?;
    Some(match error {
        NamingError::NoNamespace => {
            let known: Vec<&str> = config.namespaces.iter().map(|n| n.name.as_str()).collect();
            format!(
                "name must start with a namespace prefix; known namespaces: {}",
                known.join(", ")
            )
        }
        NamingError::Word { word } => {
            format!("`{word}` must be lowercase letters or an approved prefix")
        }
        NamingError::Prefix { prefix, namespace } => {
            format!("`{prefix}` is not an approved prefix of namespace `{namespace}`")
        }
        NamingError::Structure { namespace } => {
            format!("expected `{namespace}_<approved prefix>_<word>_<types|test|tests>`")
        }
        NamingError::TooManyParts { namespace } => {
            format!("too many parts after `{namespace}_`")
        }
    })
}

fn orphan_test_crate(krate: &Crate, names: &BTreeSet<&str>, config: &Config) -> Option<String> {
    let base = config::test_crate_base(&krate.name)?;
    let excepted = krate.namespace.is_some_and(|index| {
        config.namespaces[index]
            .test_crate_exceptions
            .contains(&krate.name)
    });
    (!excepted && !names.contains(base)).then(|| format!("test crate requires crate `{base}`"))
}

/// The reason suffix of every rule that forbids `dependency` in this crate.
fn restrictions(krate: &Crate, dependency: &str, regular: bool, config: &Config) -> Vec<String> {
    let own = krate
        .namespace
        .map(|index| config.namespaces[index].dependency_rules.as_slice())
        .unwrap_or_default();
    config
        .dependency_rules
        .iter()
        .chain(own)
        .filter(|rule| regular || !rule.regular_deps_only)
        .filter(|rule| rule.pattern.matches(dependency) && !rule.allows(&krate.name, config))
        .map(|rule| {
            if rule.reason.is_empty() {
                String::new()
            } else {
                format!(": {}", rule.reason)
            }
        })
        .collect()
}

fn unused_workspace_deps<'a>(
    workspace: &'a Workspace,
    members: &'a BTreeSet<&str>,
) -> impl Iterator<Item = Diagnostic> + 'a {
    let used: BTreeSet<&str> = workspace
        .crates
        .iter()
        .flat_map(|krate| &krate.tables)
        .flat_map(|table| &table.entries)
        .map(|entry| entry.name.as_str())
        .collect();
    workspace
        .deps
        .keys()
        .filter(move |dep| !used.contains(dep.as_str()) && !members.contains(dep.as_str()))
        .map(|dep| Diagnostic {
            krate: "workspace".to_owned(),
            rule: RuleId::UnusedWorkspaceDep,
            table: None,
            dependency: Some(dep.clone()),
            manifest: "Cargo.toml".to_owned(),
            message: format!("workspace dependency `{dep}` is not used by any crate"),
        })
}
