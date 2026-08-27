//! Item ↔ scope queries.
//!
//! The builder opens an item's scope in the same step that allocates the item, so
//! the link is recorded directly ([`Scope::owner`](baml_compiler2_hir::scope::Scope::owner)).
//! Before that, consumers recovered it by scanning for a scope whose `range`
//! equalled the item's `span` — which made item spans load-bearing *semantic
//! identity* rather than diagnostic metadata, and blocked moving them into the
//! source map. These queries replace that scan.

use baml_compiler2_hir::{
    loc::{
        ClassLoc, ClientLoc, EnumLoc, FunctionLoc, ImplLoc, InterfaceLoc, LetLoc, RetryPolicyLoc,
        TemplateStringLoc, TypeAliasLoc,
    },
    scope::{ItemScopeOwner, ScopeId},
};

/// The item a scope was opened for, as a `Loc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum ScopeOwner<'db> {
    Function(FunctionLoc<'db>),
    Class(ClassLoc<'db>),
    Enum(EnumLoc<'db>),
    Interface(InterfaceLoc<'db>),
    TypeAlias(TypeAliasLoc<'db>),
    TemplateString(TemplateStringLoc<'db>),
    Client(ClientLoc<'db>),
    RetryPolicy(RetryPolicyLoc<'db>),
    Let(LetLoc<'db>),
    Impl(ImplLoc<'db>),
}

/// The item `scope` was opened for, or `None` for structural and expression
/// scopes (Project/Package/Namespace/File, Block/Lambda/MatchArm/…).
#[salsa::tracked]
pub fn scope_owner<'db>(db: &'db dyn crate::Db, scope: ScopeId<'db>) -> Option<ScopeOwner<'db>> {
    let file = scope.file(db);
    let index = crate::file_semantic_index(db, file);
    let owner = index.scope_owner(scope.file_scope_id(db))?;

    Some(match owner {
        ItemScopeOwner::Function(id) => ScopeOwner::Function(FunctionLoc::new(db, file, id)),
        ItemScopeOwner::Class(id) => ScopeOwner::Class(ClassLoc::new(db, file, id)),
        ItemScopeOwner::Enum(id) => ScopeOwner::Enum(EnumLoc::new(db, file, id)),
        ItemScopeOwner::Interface(id) => ScopeOwner::Interface(InterfaceLoc::new(db, file, id)),
        ItemScopeOwner::TypeAlias(id) => ScopeOwner::TypeAlias(TypeAliasLoc::new(db, file, id)),
        ItemScopeOwner::TemplateString(id) => {
            ScopeOwner::TemplateString(TemplateStringLoc::new(db, file, id))
        }
        ItemScopeOwner::Client(id) => ScopeOwner::Client(ClientLoc::new(db, file, id)),
        ItemScopeOwner::RetryPolicy(id) => {
            ScopeOwner::RetryPolicy(RetryPolicyLoc::new(db, file, id))
        }
        ItemScopeOwner::Let(id) => ScopeOwner::Let(LetLoc::new(db, file, id)),
        ItemScopeOwner::Impl(id) => ScopeOwner::Impl(ImplLoc::new(db, file, id)),
    })
}

/// Declare the inverse: `<item>_scope(loc) -> Option<ScopeId>`.
macro_rules! item_scope {
    ($(#[$meta:meta])* $name:ident, $loc:ident, $owner:ident) => {
        $(#[$meta])*
        #[salsa::tracked]
        pub fn $name<'db>(db: &'db dyn crate::Db, item: $loc<'db>) -> Option<ScopeId<'db>> {
            let file = item.file(db);
            let index = crate::file_semantic_index(db, file);
            let scope = index.item_scope(ItemScopeOwner::$owner(item.id(db)))?;
            Some(index.scope_ids[scope.index() as usize])
        }
    };
}

item_scope!(
    /// The scope opened for `function`'s body.
    function_scope,
    FunctionLoc,
    Function
);
item_scope!(
    /// The scope opened for `class`'s members.
    class_scope,
    ClassLoc,
    Class
);
item_scope!(
    /// The scope opened for `interface`'s members.
    interface_scope,
    InterfaceLoc,
    Interface
);
item_scope!(
    /// The scope opened for `alias`'s right-hand side.
    type_alias_scope,
    TypeAliasLoc,
    TypeAlias
);
item_scope!(
    /// The scope owning `binding`'s initializer.
    let_scope,
    LetLoc,
    Let
);
item_scope!(
    /// The scope opened for `template`'s parameters and body.
    template_string_scope,
    TemplateStringLoc,
    TemplateString
);
item_scope!(
    /// The scope opened for an out-of-body `implement I for T` block's generic
    /// parameters. `None` for blocks without generics — they open no scope.
    impl_scope,
    ImplLoc,
    Impl
);
