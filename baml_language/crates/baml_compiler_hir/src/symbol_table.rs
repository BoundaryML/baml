//! Symbol table for name resolution.
//!
//! # Overview
//!
//! The symbol table is a registry of all named items in a project. It's built
//! by scanning all definitions (classes, enums, functions, etc.) and registering
//! their fully-qualified names. Later, when resolving user-written names, we
//! look them up in this table to find the corresponding definition.
//!
//! # Two-Phase Process
//!
//! 1. **Building**: We enumerate all items in the project. Each item has a
//!    deterministic FQN based on where it's defined (currently all items are
//!    `Namespace::Local` since BAML has no modules). We register each item's
//!    FQN -> Location mapping.
//!
//! 2. **Resolution**: When we encounter a user-written name like `MyClass`,
//!    we construct a candidate FQN (`Namespace::Local` + `"MyClass"`) and look
//!    it up in the table. If found, the name resolves to that item's location.
//!
//! # Namespaces
//!
//! Types and values are stored separately:
//! - **Types**: classes, enums, type aliases (used in type annotations)
//! - **Values**: functions (used in expressions/calls)
//!
//! This separation allows hypothetical future support for items with the same
//! name in different positions (like Rust's `struct Foo` and `fn Foo`).

use rustc_hash::FxHashMap;

use crate::{
    ClassLoc, ClientLoc, EnumLoc, FunctionLoc, GeneratorLoc, ItemId, TestLoc, TypeAliasLoc,
    file_item_tree, fqn::FullyQualifiedName, project_items,
};

/// A definition location in the symbol table.
///
/// Each variant contains the interned location where the item is defined.
/// Use the location to look up the item's full details from the `ItemTree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum Definition<'db> {
    /// A class definition.
    Class(ClassLoc<'db>),
    /// An enum definition.
    Enum(EnumLoc<'db>),
    /// A type alias definition.
    TypeAlias(TypeAliasLoc<'db>),
    /// A function definition.
    Function(FunctionLoc<'db>),
    /// A client configuration.
    Client(ClientLoc<'db>),
    /// A generator configuration.
    Generator(GeneratorLoc<'db>),
    /// A test definition.
    Test(TestLoc<'db>),
}

impl<'db> Definition<'db> {
    /// Check if this definition is a type (class, enum, or type alias).
    pub fn is_type(&self) -> bool {
        matches!(
            self,
            Definition::Class(_) | Definition::Enum(_) | Definition::TypeAlias(_)
        )
    }

    /// Check if this definition is a value (function).
    pub fn is_value(&self) -> bool {
        matches!(self, Definition::Function(_))
    }

    /// Get this definition as a class location, if it is one.
    pub fn as_class(&self) -> Option<ClassLoc<'db>> {
        match self {
            Definition::Class(loc) => Some(*loc),
            _ => None,
        }
    }

    /// Get this definition as an enum location, if it is one.
    pub fn as_enum(&self) -> Option<EnumLoc<'db>> {
        match self {
            Definition::Enum(loc) => Some(*loc),
            _ => None,
        }
    }

    /// Get this definition as a type alias location, if it is one.
    pub fn as_type_alias(&self) -> Option<TypeAliasLoc<'db>> {
        match self {
            Definition::TypeAlias(loc) => Some(*loc),
            _ => None,
        }
    }

    /// Get this definition as a function location, if it is one.
    pub fn as_function(&self) -> Option<FunctionLoc<'db>> {
        match self {
            Definition::Function(loc) => Some(*loc),
            _ => None,
        }
    }
}

/// Registry of all named items in a project.
///
/// Built by scanning all definitions and recording their FQN -> location mappings.
/// Used during resolution to validate that user-written names refer to real items.
///
/// # Example Flow
///
/// ```text
/// // Building (happens once per project):
/// class User { ... }     ->  types["local.User"] = ClassLoc(...)
/// function GetUser() ... ->  values["local.GetUser"] = FunctionLoc(...)
///
/// // Resolution (happens when type-checking):
/// field: User            ->  lookup "local.User" in types -> found ClassLoc!
/// let x = GetUser()      ->  lookup "local.GetUser" in values -> found FunctionLoc!
/// ```
#[salsa::tracked]
pub struct SymbolTable<'db> {
    /// Type definitions (classes, enums, type aliases).
    ///
    /// Searched when resolving type annotations like `class Foo { bar: MyType }`.
    #[tracked]
    #[returns(ref)]
    pub types: FxHashMap<FullyQualifiedName, Definition<'db>>,

    /// Value definitions (functions).
    ///
    /// Searched when resolving value expressions like function calls.
    #[tracked]
    #[returns(ref)]
    pub values: FxHashMap<FullyQualifiedName, Definition<'db>>,
}

impl<'db> SymbolTable<'db> {
    /// Look up a type by its fully-qualified name.
    pub fn lookup_type(
        &self,
        db: &'db dyn crate::Db,
        fqn: &FullyQualifiedName,
    ) -> Option<Definition<'db>> {
        self.types(db).get(fqn).copied()
    }

    /// Look up a value by its fully-qualified name.
    pub fn lookup_value(
        &self,
        db: &'db dyn crate::Db,
        fqn: &FullyQualifiedName,
    ) -> Option<Definition<'db>> {
        self.values(db).get(fqn).copied()
    }
}

/// Build the symbol table for a project.
///
/// Scans all top-level definitions and registers their FQN -> location mappings.
/// The result is cached by Salsa and invalidated when project items change.
#[salsa::tracked]
pub fn symbol_table<'db>(
    db: &'db dyn crate::Db,
    project: baml_workspace::Project,
) -> SymbolTable<'db> {
    let mut types: FxHashMap<FullyQualifiedName, Definition<'db>> = FxHashMap::default();
    let mut values: FxHashMap<FullyQualifiedName, Definition<'db>> = FxHashMap::default();

    let items = project_items(db, project);
    for item in items.items(db) {
        match item {
            ItemId::Class(loc) => {
                let item_tree = file_item_tree(db, loc.file(db));
                let class = &item_tree[loc.id(db)];
                let fqn = FullyQualifiedName::local(class.name.clone());
                types.insert(fqn, Definition::Class(*loc));
            }
            ItemId::Enum(loc) => {
                let item_tree = file_item_tree(db, loc.file(db));
                let enum_def = &item_tree[loc.id(db)];
                let fqn = FullyQualifiedName::local(enum_def.name.clone());
                types.insert(fqn, Definition::Enum(*loc));
            }
            ItemId::TypeAlias(loc) => {
                let item_tree = file_item_tree(db, loc.file(db));
                let alias = &item_tree[loc.id(db)];
                let fqn = FullyQualifiedName::local(alias.name.clone());
                types.insert(fqn, Definition::TypeAlias(*loc));
            }
            ItemId::Function(loc) => {
                let item_tree = file_item_tree(db, loc.file(db));
                let func = &item_tree[loc.id(db)];
                let fqn = FullyQualifiedName::local(func.name.clone());
                values.insert(fqn, Definition::Function(*loc));
            }
            // Clients, generators, and tests are not typically referenced by name
            // in user code, but we could add them to a third namespace if needed.
            ItemId::Client(_) | ItemId::Generator(_) | ItemId::Test(_) => {}
        }
    }

    SymbolTable::new(db, types, values)
}
