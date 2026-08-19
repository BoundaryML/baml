//! Namespace-level cross-file symbol aggregation.
//!
//! `namespace_items` merges `FileSymbolContributions` for all files that
//! belong to a given (package, namespace-path) pair.
//!
//! Files are sorted alphabetically by path before merging so the "first
//! definition wins" rule is deterministic. Duplicate names are recorded in
//! `NamespaceItems::conflicts` but do not prevent resolution — downstream
//! layers always see a resolved symbol (the first one).

use std::collections::BTreeSet;

use baml_base::{Name, SourceFile, Span};
use baml_compiler_diagnostics::diagnostic::{Diagnostic, DiagnosticId, DiagnosticPhase};
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::contributions::{Contribution, Definition};

/// Interned namespace identity — package name + path within package.
#[salsa::interned]
pub struct NamespaceId<'db> {
    pub package: Name,
    pub path: Vec<Name>,
}

/// One entry in a namespace conflict — the definition + its name span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConflictEntry<'db> {
    pub definition: Definition<'db>,
    pub name_span: TextRange,
}

/// A name defined more than once within a namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameConflict<'db> {
    pub name: Name,
    /// All definitions for this name, in alphabetical file order.
    pub entries: Vec<ConflictEntry<'db>>,
}

impl<'db> NameConflict<'db> {
    /// Convert to a `Diagnostic` with cross-file span annotations.
    pub fn to_diagnostic(&self, db: &'db dyn crate::Db) -> Diagnostic {
        let first = &self.entries[0];
        let rest = &self.entries[1..];

        let first_kind = first.definition.source_kind(db);
        let kinds_match = rest
            .iter()
            .all(|e| e.definition.source_kind(db) == first_kind);
        let message = if kinds_match {
            format!("duplicate {} `{}`", first_kind, self.name)
        } else {
            let kind_list: Vec<&str> = self
                .entries
                .iter()
                .map(|e| e.definition.source_kind_name(db))
                .collect();
            format!(
                "name `{}` defined {} times as: {}",
                self.name,
                self.entries.len(),
                kind_list.join(", ")
            )
        };

        let mut diag = Diagnostic::error(DiagnosticId::DuplicateName, message);

        let first_file_id = first.definition.file(db).file_id(db);
        diag = diag.with_secondary(
            Span {
                file_id: first_file_id,
                range: first.name_span,
            },
            format!(
                "first defined as {} here",
                first.definition.source_kind_name(db)
            ),
        );

        for entry in rest {
            let file_id = entry.definition.file(db).file_id(db);
            diag = diag.with_primary(
                Span {
                    file_id,
                    range: entry.name_span,
                },
                format!(
                    "duplicate {} definition",
                    entry.definition.source_kind_name(db)
                ),
            );
        }

        diag.with_phase(DiagnosticPhase::Validation)
    }
}

/// Rare/optional data for `NamespaceItems`. Heap-allocated only when
/// at least one conflict exists, avoiding allocation in the common case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceItemsExtra<'db> {
    pub conflicts: Vec<NameConflict<'db>>,
}

/// Merged symbol contributions for all files within a namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceItems<'db> {
    /// Type-namespace items (classes, enums, type aliases).
    /// First definition (alphabetically by file path) wins.
    pub types: FxHashMap<Name, Definition<'db>>,
    /// Value-namespace items (functions, clients, etc.).
    /// First definition (alphabetically by file path) wins.
    pub values: FxHashMap<Name, Definition<'db>>,
    /// Conflicts and other rare data. `None` when no conflicts exist.
    pub extra: Option<Box<NamespaceItemsExtra<'db>>>,
}

impl<'db> NamespaceItems<'db> {
    pub fn conflicts(&self) -> &[NameConflict<'db>] {
        self.extra
            .as_ref()
            .map(|e| e.conflicts.as_slice())
            .unwrap_or(&[])
    }
}

// ── salsa::Update impl ────────────────────────────────────────────────────────

/// # Safety
///
/// `NamespaceItems<'db>` contains `Definition<'db>` (Salsa interned types
/// with a database-tied lifetime). This impl allows it to be stored and
/// returned by `#[salsa::tracked(returns(ref))]` queries.
///
/// `maybe_update` uses `PartialEq` to determine whether the value changed,
/// providing proper Salsa early-cutoff for downstream queries.
#[allow(unsafe_code)]
unsafe impl salsa::Update for NamespaceItems<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and Salsa-owned.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

/// Merges `file_symbol_contributions` for all files within a namespace.
///
/// Files are sorted alphabetically by path for deterministic "first wins"
/// ordering. Keyed by `NamespaceId` so Salsa caches per namespace. Uses
/// `PartialEq` for early-cutoff: downstream queries only re-run when the
/// merged symbol set actually changes.
#[salsa::tracked(returns(ref))]
pub fn namespace_items<'db>(
    db: &'db dyn crate::Db,
    namespace_id: NamespaceId<'db>,
) -> NamespaceItems<'db> {
    let package = namespace_id.package(db);
    let ns_path = namespace_id.path(db);

    // Collect matching files from the package's own roots
    // (`package_files`), then sort alphabetically by path — so edits to
    // another package's file set never invalidate this namespace.
    let package_id = crate::package::PackageId::new(db, package);
    let mut matching_files: Vec<SourceFile> = crate::package::package_files(db, package_id)
        .iter()
        .copied()
        .filter(|file| {
            let pkg_info = crate::file_package::file_package(db, *file);
            pkg_info.namespace_path == *ns_path
        })
        .collect();
    matching_files.sort_by_key(|a| a.path(db));

    // Accumulate all contributions per name (preserving file order).
    let mut type_defs: FxHashMap<Name, Vec<Contribution<'db>>> = FxHashMap::default();
    let mut value_defs: FxHashMap<Name, Vec<Contribution<'db>>> = FxHashMap::default();

    for file in &matching_files {
        let contributions = crate::file_symbol_contributions(db, *file);
        for (name, contrib) in &contributions.types {
            type_defs.entry(name.clone()).or_default().push(*contrib);
        }
        for (name, contrib) in &contributions.values {
            value_defs.entry(name.clone()).or_default().push(*contrib);
        }
    }

    // Lookup keeps type and value declarations separate, but BAML source does
    // not: all namespace-level declarations with the same spelling contribute
    // to one conflict. Build that conflict once so mixed declaration kinds do
    // not also produce redundant type-only or value-only diagnostics.
    let mut types: FxHashMap<Name, Definition<'db>> = FxHashMap::default();
    let mut values: FxHashMap<Name, Definition<'db>> = FxHashMap::default();
    let mut conflicts: Vec<NameConflict<'db>> = Vec::new();

    let declaration_names: BTreeSet<_> =
        type_defs.keys().chain(value_defs.keys()).cloned().collect();
    for name in declaration_names {
        let mut entries: Vec<_> = type_defs
            .get(&name)
            .into_iter()
            .flatten()
            .chain(value_defs.get(&name).into_iter().flatten())
            .copied()
            .collect();

        // Tests are function-scoped (identity is functionName/testName), so
        // same-named tests for different functions remain legal.
        let all_tests = entries
            .iter()
            .all(|entry| matches!(entry.definition, Definition::Test(_)));
        if entries.len() < 2 || all_tests {
            continue;
        }

        entries.sort_by(|left, right| {
            left.definition
                .file(db)
                .path(db)
                .cmp(&right.definition.file(db).path(db))
                .then_with(|| left.name_span.start().cmp(&right.name_span.start()))
                .then_with(|| {
                    left.definition
                        .source_kind_name(db)
                        .cmp(right.definition.source_kind_name(db))
                })
        });
        conflicts.push(NameConflict {
            name,
            entries: entries
                .into_iter()
                .map(|entry| ConflictEntry {
                    definition: entry.definition,
                    name_span: entry.name_span,
                })
                .collect(),
        });
    }

    for (name, contribs) in type_defs {
        types.insert(name.clone(), contribs[0].definition);
    }
    for (name, contribs) in value_defs {
        values.insert(name.clone(), contribs[0].definition);
    }

    // Sort conflicts by name for deterministic output.
    conflicts.sort_by(|a, b| a.name.cmp(&b.name));

    let extra = if conflicts.is_empty() {
        None
    } else {
        Some(Box::new(NamespaceItemsExtra { conflicts }))
    };

    NamespaceItems {
        types,
        values,
        extra,
    }
}
