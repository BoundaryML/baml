//! Position-independent item storage for `compiler2_hir`.
//!
//! `ItemTree` stores minimal item representations keyed by name-based IDs,
//! following the same scheme as `baml_compiler_hir::item_tree`.
//! Items are indexed by name (not source position) for position-independence.

pub mod builder;
mod classes;
mod clients;
mod common;
mod enums;
mod functions;
mod interfaces;
mod lets;
mod retry_policies;
mod source_map;
mod template_strings;
mod test_items;
mod type_aliases;

use std::ops::Index;

use baml_compiler2_ast as ast;
pub use classes::*;
pub use clients::*;
pub use common::*;
pub use enums::*;
pub use functions::*;
pub use interfaces::*;
pub use lets::*;
pub use retry_policies::*;
use rustc_hash::FxHashMap;
pub use source_map::*;
pub use template_strings::*;
pub use test_items::*;
pub use type_aliases::*;

use crate::ids::{
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, ImplMarker, InterfaceMarker, LetMarker,
    LocalItemId, RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker,
};

// ── ItemTree ─────────────────────────────────────────────────────────────────

/// Position-independent item storage for a single file.
///
/// Items are stored in hash maps keyed by name-based IDs. This is a finished,
/// immutable value: everything needed to *build* it — the collision counter, the
/// source map — lives in [`ItemTreeBuilder`](builder::ItemTreeBuilder) and is
/// dropped once the tree is complete.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ItemTree {
    pub functions: FxHashMap<LocalItemId<FunctionMarker>, Function>,
    pub classes: FxHashMap<LocalItemId<ClassMarker>, Class>,
    pub enums: FxHashMap<LocalItemId<EnumMarker>, Enum>,
    pub interfaces: FxHashMap<LocalItemId<InterfaceMarker>, Interface>,
    pub type_aliases: FxHashMap<LocalItemId<TypeAliasMarker>, TypeAlias>,
    pub clients: FxHashMap<LocalItemId<ClientMarker>, Client>,
    pub tests: FxHashMap<LocalItemId<TestMarker>, Test>,
    pub template_strings: FxHashMap<LocalItemId<TemplateStringMarker>, TemplateString>,
    pub retry_policies: FxHashMap<LocalItemId<RetryPolicyMarker>, RetryPolicy>,
    pub lets: FxHashMap<LocalItemId<LetMarker>, Let>,

    /// Unified store for every `implements` block (both in-body and
    /// out-of-body), keyed by a stable `ImplMarker` id. Downstream queries
    /// (`impl_data`) read this map; `class_to_impls` / `free_impls` index it.
    pub impls: FxHashMap<LocalItemId<ImplMarker>, ImplBlock>,
    /// Index from a class to the impls whose subject is that class
    /// (`ImplSubject::InClass`), in source order. Lets "impls for class C" be
    /// answered without a scan; parallel to `Class::implements`.
    pub class_to_impls: FxHashMap<LocalItemId<ClassMarker>, Vec<LocalItemId<ImplMarker>>>,
    /// Out-of-body (`ImplSubject::Free`) impl ids in source order. Gives consumers a
    /// deterministic iteration order over free impls (the unified `impls` map is unordered) —
    /// e.g. resolving the enclosing out-of-body impl of a method.
    pub free_impls: Vec<LocalItemId<ImplMarker>>,

    /// BEP-044: for a class method declared inside an `implements I {}`
    /// block, record the unresolved interface target path. Empty for
    /// methods declared at the class level (not inside any `implements`
    /// block) and for interface default-methods themselves. Consumers
    /// resolve the path to an `InterfaceLoc` lazily so HIR construction
    /// stays independent of name resolution.
    pub method_to_iface_target: FxHashMap<LocalItemId<FunctionMarker>, ast::TypeExpr>,
    pub method_to_iface_associated_type_bindings:
        FxHashMap<LocalItemId<FunctionMarker>, Vec<ast::AssociatedTypeBindingDef>>,

    /// Method → owning item. Inverse of `Class::methods` /
    /// `Interface::default_methods` / a free impl's `ImplBlock::methods`;
    /// absent for top-level functions. See [`MethodOwner`].
    pub method_owners: FxHashMap<LocalItemId<FunctionMarker>, MethodOwner>,
}

impl ItemTree {
    /// Generic parameters of the type declaration enclosing `method` — the
    /// class's for a class method (BEP-044: a generic interface's default
    /// method likewise sees the interface's), empty for top-level functions.
    ///
    /// Also empty for a *free-impl* method: an out-of-body block's generics
    /// live on the `ImplBlock` and are threaded by the impl-specific paths,
    /// not treated as enclosing-type parameters.
    ///
    /// The single successor of the `classes.values().find(|c|
    /// c.methods.contains(…))` scans that used to be copied (divergently)
    /// across HIR, PPIR and TIR.
    pub fn enclosing_type_generic_params(
        &self,
        method: LocalItemId<FunctionMarker>,
    ) -> &[GenericParam] {
        match self.method_owners.get(&method) {
            Some(MethodOwner::Class(id)) => &self[*id].generic_params,
            Some(MethodOwner::Interface(id)) => &self[*id].generic_params,
            Some(MethodOwner::FreeImpl(_)) | None => &[],
        }
    }
}

// ── Index impls ───────────────────────────────────────────────────────────────

impl Index<LocalItemId<FunctionMarker>> for ItemTree {
    type Output = Function;
    fn index(&self, id: LocalItemId<FunctionMarker>) -> &Function {
        &self.functions[&id]
    }
}

impl Index<LocalItemId<ClassMarker>> for ItemTree {
    type Output = Class;
    fn index(&self, id: LocalItemId<ClassMarker>) -> &Class {
        &self.classes[&id]
    }
}

impl Index<LocalItemId<EnumMarker>> for ItemTree {
    type Output = Enum;
    fn index(&self, id: LocalItemId<EnumMarker>) -> &Enum {
        &self.enums[&id]
    }
}

impl Index<LocalItemId<InterfaceMarker>> for ItemTree {
    type Output = Interface;
    fn index(&self, id: LocalItemId<InterfaceMarker>) -> &Interface {
        &self.interfaces[&id]
    }
}

impl Index<LocalItemId<TypeAliasMarker>> for ItemTree {
    type Output = TypeAlias;
    fn index(&self, id: LocalItemId<TypeAliasMarker>) -> &TypeAlias {
        &self.type_aliases[&id]
    }
}

impl Index<LocalItemId<ClientMarker>> for ItemTree {
    type Output = Client;
    fn index(&self, id: LocalItemId<ClientMarker>) -> &Client {
        &self.clients[&id]
    }
}

impl Index<LocalItemId<TestMarker>> for ItemTree {
    type Output = Test;
    fn index(&self, id: LocalItemId<TestMarker>) -> &Test {
        &self.tests[&id]
    }
}

impl Index<LocalItemId<TemplateStringMarker>> for ItemTree {
    type Output = TemplateString;
    fn index(&self, id: LocalItemId<TemplateStringMarker>) -> &TemplateString {
        &self.template_strings[&id]
    }
}

impl Index<LocalItemId<RetryPolicyMarker>> for ItemTree {
    type Output = RetryPolicy;
    fn index(&self, id: LocalItemId<RetryPolicyMarker>) -> &RetryPolicy {
        &self.retry_policies[&id]
    }
}

impl Index<LocalItemId<LetMarker>> for ItemTree {
    type Output = Let;
    fn index(&self, id: LocalItemId<LetMarker>) -> &Let {
        &self.lets[&id]
    }
}
