# Name Resolution Formalization Plan

## Status: Draft

## Problem Statement

Name resolution in the BAML compiler is currently ad-hoc. `Path` is just `Vec<Name>` with no semantic information, and resolution happens piecemeal in different places. This makes it difficult to:

1. Know what an identifier refers to (local variable? import? builtin?)
2. Support future features like modules and packages
3. Provide good IDE features (go-to-definition, find-references)
4. Generate clear error messages for unresolved names

## Current State

### Current Ty Variants (`baml_tir/src/types.rs`)
```rust
pub enum Ty {
    Int, Float, String, Bool, Null,
    Image, Audio, Video, Pdf,
    Class(Name),      // <- Will change to Class(FQN)
    Enum(Name),       // <- Will change to Enum(FQN)
    Named(Name),      // <- Will be deprecated
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map { key: Box<Ty>, value: Box<Ty> },
    Union(Vec<Ty>),
    Function { params: Vec<Ty>, ret: Box<Ty> },
    Literal(LiteralValue),
    Unknown, Error, Void,
    WatchAccessor(Box<Ty>),
}
```

### Path Definition (`baml_hir/src/path.rs:12-18`)
```rust
pub struct Path {
    pub segments: Vec<Name>,
    pub kind: PathKind,  // Only `Plain` exists today
}
```

### Type Resolution (`baml_tir/src/lower.rs`)
- `lower_type_ref_validated_resolved()` checks if name is in `known_types`, `class_names`, or `enum_names`
- No scoping - types are project-global
- Returns `Ty::Error` for unknown types

### Value Resolution (`baml_tir/src/lib.rs:689-724`)
- `TypeContext::resolve_path()` walks scope stack
- Checks locals first, then globals (functions), then enum variants
- Ad-hoc handling of multi-segment paths

### Builtins (`baml_vm/src/builtins.rs`)
- String paths like `"baml.Array.length"`
- Method lookup by receiver type pattern matching
- No formal namespace structure

## Design

### Core Principle: Two Namespaces

Type names and value names live in separate namespaces with different resolution rules:

| Namespace | Contains | When Resolved | Shadowing? |
|-----------|----------|---------------|------------|
| **Type** | classes, enums, type aliases | HIR→TIR lowering | No (flat per project) |
| **Value** | functions, variables, enum variants | Type inference | Yes (lexical scopes) |

### Fully Qualified Names

```rust
// Location: baml_hir/src/fqn.rs (new file)

/// A fully-qualified name that unambiguously identifies an item.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FullyQualifiedName {
    pub namespace: Namespace,
    pub name: Name,
}

/// The namespace an item belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Compiler builtins: baml.Array.length, env.get, etc.
    /// These are "magic" - the compiler knows about them specially.
    Builtin {
        /// Path segments after the root.
        /// e.g., ["Array", "length"] for baml.Array.length
        /// e.g., ["get"] for env.get
        path: Vec<Name>,
    },

    /// Standard library items that require `baml.` prefix.
    /// e.g., baml.http.get (future)
    BamlStd {
        /// e.g., ["http", "get"] for baml.http.get
        path: Vec<Name>,
    },

    /// User-defined items in the current project.
    Local,

    /// Future: Items from explicit user modules.
    UserModule {
        module_path: Vec<Name>,
    },

    /// Future: Items from external packages.
    Package {
        package_name: Name,
        module_path: Vec<Name>,
    },
}

impl FullyQualifiedName {
    pub fn local(name: Name) -> Self {
        Self { namespace: Namespace::Local, name }
    }

    pub fn builtin(path: Vec<Name>, name: Name) -> Self {
        Self { namespace: Namespace::Builtin { path }, name }
    }
}
```

### Primitives

Primitives (`int`, `string`, `bool`, `float`, `null`, `image`, `audio`, `video`, `pdf`) are **not** given FQNs. They are language keywords handled specially by the parser and type system.

```rust
// Primitives stay as they are in Ty
pub enum Ty {
    Int,      // No FQN - it's a keyword
    String,   // No FQN - it's a keyword
    // ...

    // User-defined types get FQN
    Class(FullyQualifiedName),
    Enum(FullyQualifiedName),
    TypeAlias(FullyQualifiedName),
}
```

### Type Resolution

Type resolution happens at the HIR→TIR boundary when lowering `TypeRef` to `Ty`. It can be a Salsa query because the inputs are stable.

Since `Ty::Class(FQN)` and `Ty::Enum(FQN)` already carry the fully-qualified name, the resolution function returns `Ty` directly - no intermediate `ResolvedType` enum needed.

**Important**: Type aliases are NOT expanded during resolution. They produce `Ty::TypeAlias(FQN)` for two reasons:
1. **Error messages**: Preserves user's chosen spelling (e.g., "expected UserId" not "expected string")
2. **Recursive types**: Aliases like `type Tree = { value: int, children: Tree[] }` require indirection to avoid infinite expansion

Expansion happens later during normalization, when needed for subtype checks.

```rust
// Location: baml_tir/src/resolve.rs (new file)

/// Resolve a type path to a Ty.
///
/// This is a Salsa query - results are cached and invalidated
/// when project definitions change.
///
/// - Primitives -> Ty::Int, Ty::String, etc.
/// - Classes -> Ty::Class(FQN::local(name))
/// - Enums -> Ty::Enum(FQN::local(name))
/// - Type aliases -> Ty::TypeAlias(FQN::local(name)) (NOT expanded)
/// - Unknown -> Ty::Error
#[salsa::tracked]
pub fn resolve_type_path(
    db: &dyn Db,
    project: Project,
    path: Path,
) -> Ty {
    // 1. Check if it's a primitive
    if path.is_simple() {
        if let Some(ty) = try_resolve_primitive(path.last_segment().unwrap()) {
            return ty;
        }
    }

    // 2. Check project types (classes, enums, type aliases)
    let class_name_set = class_names(db, project);
    let enum_name_set = enum_names(db, project);
    let type_alias_names = type_alias_names(db, project);

    if let Some(name) = path.last_segment() {
        if class_name_set.names(db).contains(name) {
            return Ty::Class(FullyQualifiedName::local(name.clone()));
        }
        if enum_name_set.names(db).contains(name) {
            return Ty::Enum(FullyQualifiedName::local(name.clone()));
        }
        // Type aliases stay as TypeAlias - expanded during normalization
        if type_alias_names.names(db).contains(name) {
            return Ty::TypeAlias(FullyQualifiedName::local(name.clone()));
        }
    }

    // 3. Future: Check imports, modules, packages

    Ty::Error
}

fn try_resolve_primitive(name: &Name) -> Option<Ty> {
    match name.as_str().to_lowercase().as_str() {
        "int" => Some(Ty::Int),
        "float" => Some(Ty::Float),
        "string" => Some(Ty::String),
        "bool" => Some(Ty::Bool),
        "null" => Some(Ty::Null),
        "image" => Some(Ty::Image),
        "audio" => Some(Ty::Audio),
        "video" => Some(Ty::Video),
        "pdf" => Some(Ty::Pdf),
        _ => None,
    }
}
```

### Value Resolution

Value resolution happens during type inference and requires the scope stack. It **cannot** be a simple Salsa query because it depends on dynamic context.

```rust
// Location: baml_tir/src/resolve.rs

/// Result of resolving a value path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedValue {
    /// Local variable (from let binding or function parameter).
    /// Locals don't have FQNs - they're ephemeral.
    Local { name: Name },

    /// User-defined function.
    Function(FullyQualifiedName),

    /// Enum variant (e.g., Status.Active).
    EnumVariant {
        enum_fqn: FullyQualifiedName,
        variant: Name,
    },

    /// Builtin free function (e.g., env.get, baml.deep_copy).
    BuiltinFunction {
        /// The builtin's path as defined in builtins.rs
        path: &'static str,
    },

    /// Resolution failed.
    Error,
}

impl TypeContext<'_> {
    /// Resolve a value path in the current scope.
    ///
    /// Resolution order:
    /// 1. Local variables (innermost scope first)
    /// 2. Function parameters (in function scope)
    /// 3. Project-level functions
    /// 4. Enum variants (for two-segment paths like Status.Active)
    /// 5. Builtin functions (for paths starting with known prefixes)
    pub fn resolve_value_path(&self, segments: &[Name]) -> ResolvedValue {
        if segments.is_empty() {
            return ResolvedValue::Error;
        }

        let first = &segments[0];

        // 1. Check local variables
        if segments.len() == 1 {
            if self.lookup(first).is_some() {
                return ResolvedValue::Local { name: first.clone() };
            }
        }

        // 2. Check project functions (single-segment)
        if segments.len() == 1 {
            if self.scopes.first().and_then(|s| s.get(first)).is_some() {
                return ResolvedValue::Function(FullyQualifiedName::local(first.clone()));
            }
        }

        // 3. Check enum variants (two-segment: EnumName.Variant)
        if segments.len() == 2 {
            let enum_name = &segments[0];
            let variant_name = &segments[1];
            if let Some(variants) = self.lookup_enum_variants(enum_name) {
                if variants.contains(variant_name) {
                    return ResolvedValue::EnumVariant {
                        enum_fqn: FullyQualifiedName::local(enum_name.clone()),
                        variant: variant_name.clone(),
                    };
                }
            }
        }

        // 4. Check builtin functions
        let path_str = segments.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(".");
        if builtins::lookup_builtin_by_path(&path_str).is_some() {
            return ResolvedValue::BuiltinFunction {
                path: builtins::intern_path(&path_str),
            };
        }

        ResolvedValue::Error
    }
}
```

### Method Resolution

Method resolution is **type-directed** - it requires knowing the receiver's type. This happens during type inference, not during name resolution.

```rust
// Location: baml_tir/src/resolve.rs

/// Result of resolving a method call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedMethod {
    /// Builtin method (e.g., .length(), .push()).
    Builtin {
        /// The builtin's path (e.g., "baml.Array.length")
        path: &'static str,
        /// The concrete receiver type (e.g., Ty::List(Ty::Int))
        receiver_ty: Ty,
    },

    /// User-defined method (future: when we have impl blocks).
    UserDefined {
        impl_fqn: FullyQualifiedName,
        method_name: Name,
        receiver_ty: Ty,
    },

    /// Resolution failed.
    Error,
}

/// Resolve a method call on a known receiver type.
///
/// Called during type inference when we encounter `receiver.method(args)`
/// and have already inferred the type of `receiver`.
pub fn resolve_method(receiver_ty: &Ty, method_name: &Name) -> ResolvedMethod {
    // Try builtin methods first
    if let Some((def, _bindings)) = builtins::lookup_method(receiver_ty, method_name.as_str()) {
        return ResolvedMethod::Builtin {
            path: def.path,
            receiver_ty: receiver_ty.clone(),
        };
    }

    // Future: check user-defined methods from impl blocks

    ResolvedMethod::Error
}
```

### Symbol Table

A centralized symbol table built during HIR construction:

```rust
// Location: baml_hir/src/symbol_table.rs (new file)

/// A definition in the symbol table.
#[derive(Debug, Clone)]
pub enum Definition<'db> {
    Class(ClassLoc<'db>),
    Enum(EnumLoc<'db>),
    TypeAlias(TypeAliasLoc<'db>),
    Function(FunctionLoc<'db>),
    Client(ClientLoc<'db>),
    Generator(GeneratorLoc<'db>),
    Test(TestLoc<'db>),
}

/// Symbol table for a project.
///
/// Maps fully-qualified names to their definitions.
#[salsa::tracked]
pub struct SymbolTable<'db> {
    /// Type definitions (classes, enums, type aliases).
    #[returns(ref)]
    pub types: HashMap<FullyQualifiedName, Definition<'db>>,

    /// Value definitions (functions).
    #[returns(ref)]
    pub values: HashMap<FullyQualifiedName, Definition<'db>>,
}

/// Build the symbol table for a project.
#[salsa::tracked]
pub fn symbol_table(db: &dyn Db, project: Project) -> SymbolTable<'db> {
    let mut types = HashMap::new();
    let mut values = HashMap::new();

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
                let sig = function_signature(db, *loc);
                let fqn = FullyQualifiedName::local(sig.name.clone());
                values.insert(fqn, Definition::Function(*loc));
            }
            // Clients, generators, tests are not typically referenced by name
            // in user code, but could be added if needed
            _ => {}
        }
    }

    SymbolTable::new(db, types, values)
}
```

### Updated Ty Enum

```rust
// Location: baml_tir/src/types.rs (modifications)

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitives (no FQN - they're keywords)
    Int,
    Float,
    String,
    Bool,
    Null,
    Image,
    Audio,
    Video,
    Pdf,

    // User-defined types (with FQN)
    Class(FullyQualifiedName),
    Enum(FullyQualifiedName),
    TypeAlias(FullyQualifiedName),  // Expanded during normalization, not resolution

    // For backwards compatibility during migration, keep Named
    // Eventually remove this in favor of Class/Enum
    #[deprecated(note = "Use Class or Enum with FQN instead")]
    Named(Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map { key: Box<Ty>, value: Box<Ty> },
    Union(Vec<Ty>),

    // Functions
    Function { params: Vec<Ty>, ret: Box<Ty> },

    // Literal types
    Literal(LiteralValue),

    // Special
    Unknown,
    Error,
    Void,
    WatchAccessor(Box<Ty>),
}
```

## Implementation Roadmap

### Phase 1: Core Infrastructure

1. **Create `baml_hir/src/fqn.rs`**
   - Define `FullyQualifiedName` struct
   - Define `Namespace` enum
   - Add helper methods

2. **Create `baml_hir/src/symbol_table.rs`**
   - Define `Definition` enum
   - Define `SymbolTable` tracked struct
   - Implement `symbol_table()` query

3. **Update exports in `baml_hir/src/lib.rs`**
   - Export new types

### Phase 2: Type Resolution

4. **Update `Ty` enum in `baml_tir/src/types.rs`**
   - Change `Class(Name)` to `Class(FullyQualifiedName)`
   - Change `Enum(Name)` to `Enum(FullyQualifiedName)`
   - Add `TypeAlias(FullyQualifiedName)` variant
   - Deprecate `Named(Name)` variant

5. **Create `baml_tir/src/resolve.rs`**
   - Implement `resolve_type_path()` query (returns `Ty` directly)
   - Add `try_resolve_primitive()` helper
   - Add `type_alias_names()` query (set of alias names, similar to `class_names()`)

6. **Update `baml_tir/src/lower.rs`**
   - Use `resolve_type_path()` instead of ad-hoc checks
   - Return `Ty::Error` consistently for failures

### Phase 3: Value Resolution

7. **Add `ResolvedValue` to `baml_tir/src/resolve.rs`**
   - Define the enum
   - Implement `TypeContext::resolve_value_path()`

8. **Update `TypeContext::resolve_path()` in `baml_tir/src/lib.rs`**
   - Use new `resolve_value_path()` method
   - Return `ResolvedValue` instead of `ResolvedPath`

9. **Update expression type inference**
   - Use resolved values for better error messages
   - Track FQNs in `InferenceResult` for IDE features

### Phase 4: Method Resolution

10. **Add `ResolvedMethod` to `baml_tir/src/resolve.rs`**
    - Define the enum
    - Implement `resolve_method()` function

11. **Update `infer_field_access()` in `baml_tir/src/lib.rs`**
    - Use `resolve_method()` for method calls
    - Track resolved methods for codegen

### Phase 5: Cleanup and Migration

12. **Remove deprecated code**
    - Remove `Ty::Named` variant (replace all uses with Class/Enum)
    - Remove old `ResolvedPath` enum
    - Clean up redundant resolution code

13. **Update builtins**
    - Add `intern_path()` function for static path strings
    - Consider moving to FQN-based registration

14. **Update error messages**
    - Use FQN in "undefined type" errors
    - Suggest similar names based on symbol table

## Testing Strategy

### Unit Tests

- `fqn.rs`: Test FQN creation, equality, hashing
- `symbol_table.rs`: Test symbol table construction from various project shapes
- `resolve.rs`: Test type and value resolution in isolation

### Integration Tests

- Create test BAML files with various resolution scenarios:
  - Simple type references
  - Enum variant access
  - Method calls on different types
  - Shadowing (local over function)
  - Error cases (undefined types, unknown methods)

### Migration Tests

- Ensure existing test suite passes throughout migration
- Add regression tests for any bugs found during migration

## Future Extensions

This design supports future features:

1. **User Modules**: `Namespace::UserModule { module_path }` handles `users.User`
2. **Packages**: `Namespace::Package { package_name, module_path }` handles external deps
3. **Imports**: Symbol table can track import aliases
4. **Impl Blocks**: `ResolvedMethod::UserDefined` handles user methods
5. **IDE Features**: FQNs enable precise go-to-definition and find-references

## Open Questions

1. **Should we support type/value name collision?**
   - Rust allows `struct Foo` and `fn Foo()` to coexist
   - Current BAML probably doesn't - need to verify

2. **How to handle forward references?**
   - Class A references Class B which is defined later in the file
   - Currently works because we build all types before resolving
   - Symbol table approach preserves this

3. **Interning for FQNs?**
   - FQNs will be compared frequently
   - Could use Salsa interning for O(1) equality
   - May be premature optimization for now
