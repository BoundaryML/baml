//! Position-independent item storage.
//!
//! The `ItemTree` contains minimal representations of all items in a container.
//! It acts as an "invalidation barrier" - only changes to item signatures
//! cause the `ItemTree` to change, not edits to whitespace, comments, or bodies.

use std::ops::Index;

use baml_base::Name;
use rustc_hash::FxHashMap;

use crate::{
    ids::{ItemKind, LocalItemId, hash_name},
    loc::{ClassMarker, ClientMarker, EnumMarker, FunctionMarker, TestMarker, TypeAliasMarker},
    type_ref::TypeRef,
};

/// Position-independent item storage for a container.
///
/// This is the core HIR data structure. Items are stored in hash maps
/// keyed by name-based IDs, following rust-analyzer's architecture.
///
/// **Key property:** Items are indexed by name, not source position.
/// Adding an item in the middle of the file doesn't change the `LocalItemIds`
/// of other items because `LocalItemIds` are derived from names.
///
/// **Collision handling:** IDs pack a 16-bit hash and 16-bit collision index.
/// The `next_index` map tracks the next available index per `(ItemKind, hash)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemTree {
    pub(crate) functions: FxHashMap<LocalItemId<FunctionMarker>, Function>,
    pub(crate) classes: FxHashMap<LocalItemId<ClassMarker>, Class>,
    pub(crate) enums: FxHashMap<LocalItemId<EnumMarker>, Enum>,
    pub(crate) type_aliases: FxHashMap<LocalItemId<TypeAliasMarker>, TypeAlias>,
    pub(crate) clients: FxHashMap<LocalItemId<ClientMarker>, Client>,
    pub(crate) tests: FxHashMap<LocalItemId<TestMarker>, Test>,

    /// Collision tracker: (`ItemKind`, hash) -> next available index.
    /// Single map for all item types, following rust-analyzer's pattern.
    next_index: FxHashMap<(ItemKind, u16), u16>,
}

impl Default for ItemTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemTree {
    /// Create a new empty `ItemTree`.
    pub fn new() -> Self {
        Self {
            functions: FxHashMap::default(),
            classes: FxHashMap::default(),
            enums: FxHashMap::default(),
            type_aliases: FxHashMap::default(),
            clients: FxHashMap::default(),
            tests: FxHashMap::default(),
            next_index: FxHashMap::default(),
        }
    }

    /// Allocate a collision-resistant ID for an item.
    /// Returns a `LocalItemId` with the name's hash and a unique collision index.
    fn alloc_id<T>(&mut self, kind: ItemKind, name: &Name) -> LocalItemId<T> {
        let hash = hash_name(name);
        let index = self.next_index.entry((kind, hash)).or_insert(0);
        let id = LocalItemId::new(hash, *index);
        *index += 1;
        id
    }

    /// Add a function and return its local ID.
    pub fn alloc_function(&mut self, func: Function) -> LocalItemId<FunctionMarker> {
        let id = self.alloc_id(ItemKind::Function, &func.name);
        self.functions.insert(id, func);
        id
    }

    /// Add a class and return its local ID.
    pub fn alloc_class(&mut self, class: Class) -> LocalItemId<ClassMarker> {
        let id = self.alloc_id(ItemKind::Class, &class.name);
        self.classes.insert(id, class);
        id
    }

    /// Add an enum and return its local ID.
    pub fn alloc_enum(&mut self, enum_def: Enum) -> LocalItemId<EnumMarker> {
        let id = self.alloc_id(ItemKind::Enum, &enum_def.name);
        self.enums.insert(id, enum_def);
        id
    }

    /// Add a type alias and return its local ID.
    pub fn alloc_type_alias(&mut self, alias: TypeAlias) -> LocalItemId<TypeAliasMarker> {
        let id = self.alloc_id(ItemKind::TypeAlias, &alias.name);
        self.type_aliases.insert(id, alias);
        id
    }

    /// Add a client and return its local ID.
    pub fn alloc_client(&mut self, client: Client) -> LocalItemId<ClientMarker> {
        let id = self.alloc_id(ItemKind::Client, &client.name);
        self.clients.insert(id, client);
        id
    }

    /// Add a test and return its local ID.
    pub fn alloc_test(&mut self, test: Test) -> LocalItemId<TestMarker> {
        let id = self.alloc_id(ItemKind::Test, &test.name);
        self.tests.insert(id, test);
        id
    }
}

/// A function definition in the `ItemTree`.
///
/// This is the MINIMAL representation - ONLY the name.
/// Everything else (params, return type, body) is in separate queries for incrementality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Name,
}

/// A class definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: Name,
    pub fields: Vec<Field>,

    /// Block attributes (@@dynamic, @@alias, etc.).
    pub is_dynamic: bool,
    // Note: Generic parameters are queried separately via generic_params()
    // for incrementality - changes to generics don't invalidate ItemTree
}

/// Class field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: Name,
    pub type_ref: TypeRef,
}

/// An enum definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Name,
    pub variants: Vec<EnumVariant>,
    // Note: Generic parameters are queried separately via generic_params()
}

/// Enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Name,
}

/// Type alias definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Name,
    pub type_ref: TypeRef,
    // Note: Generic parameters are queried separately via generic_params()
}

/// Client configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub name: Name,
    pub provider: Name,
}

/// Test definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub name: Name,

    /// Unresolved function references.
    pub function_refs: Vec<Name>,
}

//
// ──────────────────────────────────────────────────────── INDEX IMPLS ─────
//

/// Index `ItemTree` by `FunctionMarker` to get Function data.
impl Index<LocalItemId<FunctionMarker>> for ItemTree {
    type Output = Function;
    fn index(&self, index: LocalItemId<FunctionMarker>) -> &Self::Output {
        self.functions
            .get(&index)
            .expect("Function not found in ItemTree")
    }
}

/// Index `ItemTree` by `ClassMarker` to get Class data.
impl Index<LocalItemId<ClassMarker>> for ItemTree {
    type Output = Class;
    fn index(&self, index: LocalItemId<ClassMarker>) -> &Self::Output {
        self.classes
            .get(&index)
            .expect("Class not found in ItemTree")
    }
}

/// Index `ItemTree` by `EnumMarker` to get Enum data.
impl Index<LocalItemId<EnumMarker>> for ItemTree {
    type Output = Enum;
    fn index(&self, index: LocalItemId<EnumMarker>) -> &Self::Output {
        self.enums.get(&index).expect("Enum not found in ItemTree")
    }
}

/// Index `ItemTree` by `TypeAliasMarker` to get `TypeAlias` data.
impl Index<LocalItemId<TypeAliasMarker>> for ItemTree {
    type Output = TypeAlias;
    fn index(&self, index: LocalItemId<TypeAliasMarker>) -> &Self::Output {
        self.type_aliases
            .get(&index)
            .expect("TypeAlias not found in ItemTree")
    }
}

/// Index `ItemTree` by `ClientMarker` to get Client data.
impl Index<LocalItemId<ClientMarker>> for ItemTree {
    type Output = Client;
    fn index(&self, index: LocalItemId<ClientMarker>) -> &Self::Output {
        self.clients
            .get(&index)
            .expect("Client not found in ItemTree")
    }
}

/// Index `ItemTree` by `TestMarker` to get Test data.
impl Index<LocalItemId<TestMarker>> for ItemTree {
    type Output = Test;
    fn index(&self, index: LocalItemId<TestMarker>) -> &Self::Output {
        self.tests.get(&index).expect("Test not found in ItemTree")
    }
}
