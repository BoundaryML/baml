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
    loc::{
        ClassMarker, ClientMarker, EnumMarker, FunctionMarker, GeneratorMarker, TestMarker,
        TypeAliasMarker,
    },
    type_ref::TypeRef,
};

//
// ──────────────────────────────────────────────────────── ATTRIBUTE TYPE ─────
//

/// Represents an attribute that may or may not be explicitly set in source.
///
/// This generic type captures whether an attribute was present in the BAML source.
/// `T` is the value type: `String` for attributes like `@alias("name")`,
/// or `()` for presence-only attributes like `@@dynamic`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Attribute<T> {
    /// Attribute was not present in source.
    #[default]
    Unset,
    /// Attribute was explicitly set with the given value.
    Explicit(T),
}

impl<T> Attribute<T> {
    /// Returns the value if explicitly set, None otherwise.
    pub fn value(&self) -> Option<&T> {
        match self {
            Attribute::Unset => None,
            Attribute::Explicit(v) => Some(v),
        }
    }

    /// Returns true if the attribute was explicitly set.
    pub fn is_explicit(&self) -> bool {
        matches!(self, Attribute::Explicit(_))
    }
}

//
// ──────────────────────────────────────────────────────── ITEM TREE ─────
//

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
    pub(crate) generators: FxHashMap<LocalItemId<GeneratorMarker>, Generator>,
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
            generators: FxHashMap::default(),
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

    /// Add a generator and return its local ID.
    pub fn alloc_generator(&mut self, generator: Generator) -> LocalItemId<GeneratorMarker> {
        let id = self.alloc_id(ItemKind::Generator, &generator.name);
        self.generators.insert(id, generator);
        id
    }

    /// Iterate over all classes in the item tree.
    pub fn iter_classes(&self) -> impl Iterator<Item = (&LocalItemId<ClassMarker>, &Class)> {
        self.classes.iter()
    }
}

/// Metadata for compiler-generated functions.
///
/// These functions are created by the compiler during HIR lowering,
/// not by the user. They skip type inference and have special handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompilerGenerated {
    /// Client resolve function - evaluates options and returns `PrimitiveClient`.
    /// Contains the client name (e.g., "GPT4" for "GPT4.resolve").
    ClientResolve { client_name: Name },
}

/// A function definition in the `ItemTree`.
///
/// This is the MINIMAL representation - ONLY the name.
/// Everything else (params, return type, body) is in separate queries for incrementality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Name,
    /// If this function is compiler-generated, contains the metadata.
    /// `None` for user-defined functions.
    pub compiler_generated: Option<CompilerGenerated>,
}

/// A class definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: Name,
    pub fields: Vec<Field>,

    // Block attributes (@@dynamic, @@alias, @@description)
    /// @@dynamic - marks class as dynamically extensible
    pub is_dynamic: Attribute<()>,
    /// @@alias("name") - alternative name for serialization
    pub alias: Attribute<String>,
    /// @@description("text") - documentation for the class
    pub description: Attribute<String>,
    // Note: Generic parameters are queried separately via generic_params()
    // for incrementality - changes to generics don't invalidate ItemTree
}

/// Class field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: Name,
    pub type_ref: TypeRef,

    // Field attributes (@alias, @description, @skip)
    /// @alias("name") - alternative name for serialization
    pub alias: Attribute<String>,
    /// @description("text") - documentation for the field
    pub description: Attribute<String>,
    /// @skip - exclude field from serialization
    pub skip: Attribute<()>,
}

/// An enum definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Name,
    pub variants: Vec<EnumVariant>,

    // Block attributes (@@alias)
    /// @@alias("name") - alternative name for serialization
    pub alias: Attribute<String>,
    // Note: Generic parameters are queried separately via generic_params()
}

/// Enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Name,

    // Variant attributes (@alias, @description, @skip)
    /// @alias("name") - alternative name for serialization
    pub alias: Attribute<String>,
    /// @description("text") - documentation for the variant
    pub description: Attribute<String>,
    /// @skip - exclude variant from serialization
    pub skip: Attribute<()>,
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
    /// Default role for chat messages (e.g., "user").
    pub default_role: Option<String>,
    /// Allowed roles for chat messages.
    pub allowed_roles: Vec<String>,
}

/// Test definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub name: Name,

    /// Unresolved function references.
    pub function_refs: Vec<Name>,

    /// Type builder block containing dynamic type definitions.
    pub type_builder: Option<TypeBuilderBlock>,
}

/// A `type_builder` block inside a test definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeBuilderBlock {
    pub entries: Vec<TypeBuilderEntry>,
}

/// An entry in a `type_builder` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeBuilderEntry {
    /// A class definition (non-dynamic).
    Class(Class),
    /// An enum definition (non-dynamic).
    Enum(Enum),
    /// A dynamic class definition.
    DynamicClass(Class),
    /// A dynamic enum definition.
    DynamicEnum(Enum),
    /// A type alias.
    TypeAlias(TypeAlias),
}

/// Generator configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generator {
    pub name: Name,
    /// The output type (e.g., "python/pydantic", "typescript").
    pub output_type: Option<String>,
    /// The output directory (relative to `baml_src`).
    pub output_dir: Option<String>,
    /// The version string.
    pub version: Option<String>,
    /// Default client mode: "sync" or "async".
    pub default_client_mode: Option<String>,
    /// Command to run after code generation.
    pub on_generate: Option<String>,
    /// Project identifier for boundary-cloud.
    pub project: Option<String>,
    /// Go package name (required for Go generator).
    pub client_package_name: Option<String>,
    /// Module format for TypeScript: "cjs" or "esm".
    pub module_format: Option<String>,
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

/// Index `ItemTree` by `GeneratorMarker` to get Generator data.
impl Index<LocalItemId<GeneratorMarker>> for ItemTree {
    type Output = Generator;
    fn index(&self, index: LocalItemId<GeneratorMarker>) -> &Self::Output {
        self.generators
            .get(&index)
            .expect("Generator not found in ItemTree")
    }
}
