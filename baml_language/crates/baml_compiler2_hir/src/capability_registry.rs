//! LLM capability registry (`_plan/llm-desugar-capabilities-plan.md` §1.2).
//!
//! Unions the two intrinsic markers across every file the compile session can
//! see (builtin stdlib + user package):
//!
//! - `//baml:llm_capability` interfaces — the registered capabilities;
//! - `//baml:llm_companion(<suffix>)` functions — the drivers behind the
//!   generated `Foo$<suffix>` companions of LLM declarative functions.
//!
//! The collection here is **purely syntactic**: no type resolution and no
//! validation. The semantic checks (marker on an interface that transitively
//! `requires baml.ai.Provider`, conforming driver signature, duplicate or
//! stdlib-shadowing suffixes) belong to TIR, where types exist; consumers
//! that need signature detail re-read it from the item tree via
//! `(file, item)`.

use baml_base::{Name, SourceFile};
use text_size::TextRange;

use crate::ids::{FunctionMarker, InterfaceMarker, LocalItemId};

/// An interface registered as an LLM capability via `//baml:llm_capability`.
#[derive(Clone, PartialEq, Eq)]
pub struct RegisteredCapability {
    pub name: Name,
    /// Owning package (`"baml"` for stdlib, `"user"` for project code).
    pub package: Name,
    pub namespace_path: Vec<Name>,
    pub file: SourceFile,
    pub item: LocalItemId<InterfaceMarker>,
    pub span: TextRange,
}

/// A driver function registered via `//baml:llm_companion(<suffix>)`.
#[derive(Clone, PartialEq, Eq)]
pub struct CompanionDriver {
    /// The companion suffix: `Foo$<suffix>` delegates to this driver.
    pub suffix: Name,
    /// The driver function's own name (e.g. `drive_stream`).
    pub function: Name,
    pub package: Name,
    pub namespace_path: Vec<Name>,
    pub file: SourceFile,
    pub item: LocalItemId<FunctionMarker>,
    /// Generic arity as declared. The convention is name-based (`T` = return
    /// slot, `TPartial` = stream-expanded slot, others passthrough); this is
    /// the raw count, recorded for consumers — validation lives in the check
    /// pass (E0151).
    pub generic_arity: usize,
    pub span: TextRange,
}

/// The unioned registry for a compile session.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct CapabilityRegistry {
    pub capabilities: Vec<RegisteredCapability>,
    pub drivers: Vec<CompanionDriver>,
}

impl CapabilityRegistry {
    /// Look up a driver by companion suffix. First declaration (in the
    /// deterministic file order of [`capability_registry`]) wins; duplicate
    /// suffixes are a TIR diagnostic, not resolved here.
    pub fn driver_for_suffix(&self, suffix: &str) -> Option<&CompanionDriver> {
        self.drivers.iter().find(|d| d.suffix.as_str() == suffix)
    }
}

/// # Safety
///
/// Mirrors `PackageItems`'s impl: no Salsa-interned (`'db`) data inside, so
/// storing by value is sound; `maybe_update` uses `PartialEq` for early-cutoff.
#[allow(unsafe_code)]
unsafe impl salsa::Update for CapabilityRegistry {
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

/// Collect the capability registry across all files (builtins + user).
///
/// Memoized per project ([`capability_registry_tracked`]): the walk touches
/// every file's item tree, and both the check pass and PPIR expansion consult
/// it per file — unmemoized that is O(files²).
pub fn capability_registry(db: &dyn crate::Db) -> &CapabilityRegistry {
    capability_registry_tracked(db, db.project())
}

#[salsa::tracked(returns(ref))]
fn capability_registry_tracked(
    db: &dyn crate::Db,
    _project: baml_workspace::Project,
) -> CapabilityRegistry {
    let mut files = crate::compiler2_all_files(db);
    files.sort_by_key(|f| f.path(db));

    let mut registry = CapabilityRegistry::default();
    for file in files {
        let pkg = crate::file_package::file_package(db, file);
        let tree = crate::file_item_tree(db, file);

        let mut ifaces: Vec<_> = tree
            .interfaces
            .iter()
            .filter(|(_, i)| i.is_llm_capability)
            .collect();
        ifaces.sort_by_key(|(_, i)| i.span.start());
        for (id, iface) in ifaces {
            registry.capabilities.push(RegisteredCapability {
                name: iface.name.clone(),
                package: pkg.package.clone(),
                namespace_path: pkg.namespace_path.clone(),
                file,
                item: *id,
                span: iface.span,
            });
        }

        let mut funcs: Vec<_> = tree
            .functions
            .iter()
            .filter(|(_, f)| f.llm_companion_suffix.is_some())
            .collect();
        funcs.sort_by_key(|(_, f)| f.span.start());
        for (id, func) in funcs {
            let suffix = func
                .llm_companion_suffix
                .clone()
                .expect("filtered on llm_companion_suffix.is_some()");
            registry.drivers.push(CompanionDriver {
                suffix,
                function: func.name.clone(),
                package: pkg.package.clone(),
                namespace_path: pkg.namespace_path.clone(),
                file,
                item: *id,
                generic_arity: func.generic_params.len(),
                span: func.span,
            });
        }
    }
    registry
}
