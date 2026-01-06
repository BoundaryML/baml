# Plan: Reorganize BAML Db Traits (ty-inspired)

## Current State

BAML's Db trait hierarchy:

```
salsa::Database
    |
    v
baml_hir::Db (has: project())
    |
    v
baml_tir::Db (empty marker trait)
    |
    +---> baml_vir::Db (empty marker trait)
    +---> baml_mir::Db (empty marker trait)
```

**Problems:**
1. `baml_tir::Db`, `baml_vir::Db`, `baml_mir::Db` are empty marker traits with no methods
2. Many functions take `db: &dyn Db` as the first parameter but could be methods on `Db`
3. No clear separation between "source-level" concerns and "semantic" concerns
4. The `project()` method is on `baml_hir::Db` but it's really a source/workspace concern

## ty's Architecture (Inspiration)

ty uses a clear hierarchical Db pattern:

```
salsa::Database
    |
    v
ruff_db::Db (SourceDb)
    - vendored() -> &VendoredFileSystem
    - system() -> &dyn System
    - files() -> &Files
    - python_version() -> PythonVersion

    |
    v
ty_python_semantic::Db (SemanticDb)
    - should_check_file(file: File) -> bool
    - rule_selection(file: File) -> &RuleSelection
    - lint_registry() -> &LintRegistry
    - verbose() -> bool

    |
    v
ty_project::Db (ProjectDb)
    - project() -> Project
    - dyn_clone() -> Box<dyn Db>
```

**Key patterns:**
1. Each layer adds **meaningful methods**, not just marker traits
2. Methods represent **capabilities** of that compilation phase
3. The hierarchy naturally reflects dependency: semantic needs source, project needs semantic
4. Free functions are used for Salsa-tracked queries, but configuration/context methods go on `Db`

## Proposed New Architecture

### Phase 1: Split Source and HIR Concerns

Create a new `baml_source` or use `baml_workspace` as the base Db:

```
salsa::Database
    |
    v
baml_workspace::Db (SourceDb)
    - project() -> Project           // Moved from baml_hir
    - source_files() -> &[SourceFile]
    - file_text(file: SourceFile) -> &str

    |
    v
baml_hir::Db (HirDb)
    // Methods for HIR-level queries that don't fit as Salsa queries
    // or are convenience methods over multiple queries
```

### Phase 2: Enrich baml_tir::Db with Methods

Convert the helper functions in `baml_tir` to trait methods:

```rust
#[salsa::db]
pub trait Db: baml_hir::Db {
    // Type resolution context - could be a method returning a cached context
    fn type_resolution_context(&self, project: Project) -> &TypeResolutionContext;

    // Common type lookups that are currently free functions
    fn class_ids(&self, project: Project) -> &HashMap<Name, ClassId>;
    fn enum_ids(&self, project: Project) -> &HashMap<Name, EnumId>;
    fn known_types(&self, project: Project) -> &HashSet<Name>;
}
```

**Note:** Functions returning `Ty<'db>` cannot easily become methods because the lifetime is tied to `&self`. This is why ty keeps similar functions as free functions too. However, we can still add methods for non-`Ty` queries.

### Phase 3: Add Methods to baml_vir::Db and baml_mir::Db

```rust
// baml_vir/src/lib.rs
#[salsa::db]
pub trait Db: baml_tir::Db {
    // Validation/checking methods
    fn should_validate_file(&self, file: SourceFile) -> bool;
}

// baml_mir/src/lib.rs
#[salsa::db]
pub trait Db: baml_tir::Db {
    // MIR-specific configuration
    fn optimization_level(&self) -> OptLevel;
}
```

## Detailed Implementation Steps

### Step 1: Audit Current `db` Parameter Usage

Categorize all functions taking `db: &dyn Db`:

| Function | Crate | Returns `Ty<'db>`? | Candidate for Method? |
|----------|-------|-------------------|----------------------|
| `typing_context` | tir | Yes | No (lifetime issue) |
| `class_field_types` | tir | Yes | No (lifetime issue) |
| `class_ids` | tir | No | **Yes** |
| `enum_ids` | tir | No | **Yes** |
| `known_types` | tir | No | **Yes** |
| `type_aliases` | tir | Yes | No (lifetime issue) |

### Step 2: Move `project()` to baml_workspace::Db

1. Create `baml_workspace::Db` trait extending `salsa::Database`
2. Move `project()` method there
3. Make `baml_hir::Db` extend `baml_workspace::Db`
4. Update all implementations

```rust
// baml_workspace/src/lib.rs
#[salsa::db]
pub trait Db: salsa::Database {
    fn project(&self) -> Project;
}

// baml_hir/src/lib.rs
#[salsa::db]
pub trait Db: baml_workspace::Db {
    // HIR-specific methods (if any)
}
```

### Step 3: Add Cached Query Methods to baml_tir::Db

For queries that don't return `Ty<'db>`, create cached helper methods:

```rust
// baml_tir/src/lib.rs

/// Cached class ID lookup
#[salsa::tracked]
pub struct ClassIds<'db> {
    #[tracked]
    #[returns(ref)]
    pub ids: HashMap<Name, baml_hir::ClassId<'db>>,
}

#[salsa::tracked]
pub fn class_ids_query(db: &dyn Db, project: Project) -> ClassIds<'_> {
    // ... implementation
}

// Extension trait for convenience
pub trait DbExt: Db {
    fn class_ids(&self, project: Project) -> &HashMap<Name, baml_hir::ClassId<'_>> {
        class_ids_query(self, project).ids(self)
    }
}

impl<T: Db + ?Sized> DbExt for T {}
```

### Step 4: Consider a Type Interning Layer

To make `Ty` work better with Salsa, consider interning types:

```rust
// Future consideration - intern Ty for better Salsa integration
#[salsa::interned]
pub struct InternedTy<'db> {
    pub data: TyData<'db>,
}

pub enum TyData<'db> {
    Int,
    Float,
    String,
    // ... etc, but inner types reference InternedTy
    List(InternedTy<'db>),
}
```

This would allow `Ty` to be used in tracked structs and as query results, enabling more methods on `Db`. This is a larger refactor but matches how ty handles types.

### Step 5: Create a Unified Database Implementation

Create a main `BamlDatabase` struct (like `ProjectDatabase` in ty):

```rust
// baml_driver/src/db.rs or baml_lsp/src/db.rs

#[salsa::db]
pub struct BamlDatabase {
    storage: salsa::Storage<Self>,
    project: Project,
    // ... other state
}

#[salsa::db]
impl baml_workspace::Db for BamlDatabase {
    fn project(&self) -> Project {
        self.project
    }
}

#[salsa::db]
impl baml_hir::Db for BamlDatabase {}

#[salsa::db]
impl baml_tir::Db for BamlDatabase {}

#[salsa::db]
impl baml_vir::Db for BamlDatabase {}

#[salsa::db]
impl baml_mir::Db for BamlDatabase {}

#[salsa::db]
impl salsa::Database for BamlDatabase {}
```

## Migration Strategy

1. **Phase 1 (Non-breaking):** Add new methods alongside existing free functions
2. **Phase 2 (Soft deprecation):** Mark free functions as deprecated, pointing to trait methods
3. **Phase 3 (Breaking):** Remove deprecated free functions in next major version

## Benefits

1. **Discoverability:** Methods on `dyn Db` are easier to find than scattered free functions
2. **Type safety:** The trait hierarchy ensures you have the right capabilities
3. **Caching:** Methods can return references to cached data without recomputation
4. **Consistency with ty:** Developers familiar with ruff/ty will find this familiar
5. **IDE support:** Better autocomplete on `db.` vs searching for free functions

## Non-Goals

- We're NOT trying to make everything a method - Salsa tracked queries remain functions
- We're NOT interning `Ty` in this first pass (could be future work)
- We're NOT changing the fundamental HIR -> TIR -> VIR -> MIR pipeline

## Open Questions

1. Should `baml_workspace::Db` also provide `files()` accessor like ruff_db?
2. Should we add `verbose()` or similar config methods to the Db traits?
3. Is it worth interning `Ty` to enable more Salsa integration?
4. Should `baml_vir::Db` and `baml_mir::Db` remain separate or merge (they both extend `baml_tir::Db`)?

## Files to Modify

- `crates/baml_workspace/src/lib.rs` - Add `Db` trait with `project()`
- `crates/baml_hir/src/lib.rs` - Extend `baml_workspace::Db`, remove `project()`
- `crates/baml_tir/src/lib.rs` - Add methods for `class_ids`, `enum_ids`, `known_types`
- `crates/baml_vir/src/lib.rs` - Add meaningful methods or document why empty
- `crates/baml_mir/src/lib.rs` - Add meaningful methods or document why empty
- All files that implement `Db` traits (TestDb, etc.)
