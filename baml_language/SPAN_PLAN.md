# BAML Span Tracking Implementation Plan

This plan implements span tracking for BAML following rust-analyzer's source map pattern, as outlined in FINDINGS.md.

---

## Current Status (Updated 2026-01-23)

### Completed Phases

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1 | ✅ Complete | ID types already existed (ExprId, StmtId, PatternId, MatchArmId) |
| Phase 2 | ✅ Complete | Created `HirSourceMap` in `baml_compiler_hir/src/source_map.rs` |
| Phase 3 | ✅ Complete | Moved spans from `ExprBody` to `HirSourceMap`, updated `FunctionBody::Expr` to be tuple |
| Phase 4 | ✅ Complete | Created `SignatureSourceMap`, removed spans from `FunctionSignature`/`Param` |
| Phase 5 | ✅ Complete | Removed unused span from `TypeContext.return_types` |
| Phase 6 | 🔲 Pending | Migrate `TypeError` to use IDs instead of spans |
| Phase 7 | 🔲 Pending | Migrate VIR and MIR |
| Phase 8 | 🔲 Pending | Final call site updates |

### Key Changes Made

1. **HIR Source Map** (`baml_compiler_hir/src/source_map.rs`):
   - `HirSourceMap`: Stores `expr_spans`, `stmt_spans`, `pattern_spans`, `match_arm_spans`
   - `SignatureSourceMap`: Stores `return_type_span`, `param_spans`

2. **FunctionBody** now returns tuple: `FunctionBody::Expr(ExprBody, HirSourceMap)`

3. **FunctionSignature** query now returns tuple: `(Arc<FunctionSignature>, SignatureSourceMap)`

4. **TypeContext** simplified: `return_types: Vec<Ty>` (removed unused span)

### Remaining Work

The main remaining issue for incremental caching is **Phase 6**: `TypeError` stores `Span` directly, and these errors end up in `InferenceResult.errors` which is Salsa-cached. To achieve full incrementality (whitespace changes don't invalidate type checking), `TypeError` needs to store IDs instead of spans.

### All Tests Passing
347+ tests across the workspace pass after these changes.

---

## Goal

Add accurate source location tracking to type errors and diagnostics without breaking Salsa's incremental compilation cache. Whitespace and comment changes should not invalidate type checking.

## Overview

The key insight from rust-analyzer: separate **what** something is (position-independent) from **where** it is (spans). Type checking operates on the "what", diagnostics resolve the "where" only at render time.

```
ItemTree (position-independent, cached)
    ↓
Type checking (uses ItemTree only)
    ↓
Errors with TypeRefId (not spans)
    ↓
Render time: TypeRefId → SourceMap → span
```

---

## Current State: Existing Span Usages

The codebase currently embeds spans directly in semantic structures. These need to be cleaned up:

### HIR Layer (`baml_compiler_hir`)

| File | Struct | Span Field(s) | Action |
|------|--------|---------------|--------|
| `body.rs` | `ExprBody` | `expr_spans: HashMap<ExprId, Span>` | **Move to SourceMap** |
| `body.rs` | `ExprBody` | `stmt_spans: HashMap<StmtId, Span>` | **Move to SourceMap** |
| `body.rs` | `ExprBody` | `pattern_spans: HashMap<PatId, Span>` | **Move to SourceMap** |
| `body.rs` | `ExprBody` | `match_arm_spans: HashMap<ExprId, Vec<MatchArmSpans>>` | **Move to SourceMap** |
| `signature.rs` | `FunctionSignature` | `return_type_span: Option<TextRange>` | **Convert to TypeRefId** |
| `signature.rs` | `Param` | `span: Option<TextRange>` | **Convert to ParamId** |

### TIR Layer (`baml_compiler_tir`)

| File | Struct | Span Field(s) | Action |
|------|--------|---------------|--------|
| `lower.rs` | `TypeLoweringContext` | `span: Option<Span>` | **Convert to TypeRefId** |
| `lib.rs` | `TypeContext` | `return_types: Vec<(Ty, Span)>` | **Convert to Vec<(Ty, ExprId)>** |

### Diagnostics (`baml_compiler_diagnostics`)

| File | Struct | Span Field(s) | Action |
|------|--------|---------------|--------|
| `type_error.rs` | `TypeError<T>` | `span: Span` on all variants | **Convert to ID + File** |
| `type_error.rs` | `TypeError<T>` | `info_span: Option<Span>` | **Convert to Option<(ID, File)>** |
| `hir_diagnostic.rs` | `HirDiagnostic` | Multiple `span`, `first_span`, `second_span` | **Convert to IDs** |

### VIR Layer (`baml_compiler_vir`)

| File | Struct | Span Field(s) | Action |
|------|--------|---------------|--------|
| `expr.rs` | `ExprBody` | `expr_spans: FxHashMap<ExprId, TextRange>` | **Move to SourceMap** |
| `lower.rs` | `LoweringError` | `span: Option<TextRange>` | **Convert to ExprId** |

### MIR Layer (`baml_compiler_mir`)

| File | Struct | Span Field(s) | Action |
|------|--------|---------------|--------|
| `ir.rs` | `MirFunction` | `span: Option<TextRange>` | **Keep or convert to FunctionId** |
| `ir.rs` | `LocalDecl` | `span: Option<TextRange>` | **Convert to LocalId** |
| `ir.rs` | `BasicBlock` | `span: Option<TextRange>` | **Keep for debugging** |
| `ir.rs` | `Statement` | `span: Option<TextRange>` | **Convert to StmtId** |

---

## Phase 1: Define ID Types and Arenas

### 1.1 Define Core ID Types

Create arena index types for all syntax elements that need source tracking:

```rust
// In a new crate or baml_base
use la_arena::{Arena, Idx};

// Type references (e.g., `int`, `string[]`, `MyClass`)
pub type TypeRefId = Idx<TypeRef>;

// Expressions
pub type ExprId = Idx<Expr>;  // Already exists in HIR

// Statements
pub type StmtId = Idx<Stmt>;  // Already exists in HIR

// Patterns
pub type PatId = Idx<Pattern>;  // Already exists in HIR

// Parameters
pub type ParamId = Idx<Param>;

// Match arms
pub type MatchArmId = Idx<MatchArm>;
```

### 1.2 Add Arenas to ItemTree/HIR

```rust
pub struct ItemTree {
    // ... existing fields ...
    type_refs: Arena<TypeRef>,
}

pub struct ExprBody {
    pub exprs: Arena<Expr>,
    pub stmts: Arena<Stmt>,
    pub patterns: Arena<Pattern>,
    pub match_arms: Arena<MatchArm>,  // New arena for match arms
    pub root_expr: Option<ExprId>,
    pub diagnostics: Vec<HirDiagnostic>,
    // REMOVED: expr_spans, stmt_spans, pattern_spans, match_arm_spans
}
```

**Tasks:**
- [x] ~~Define `TypeRefId`, `ParamId`, `MatchArmId`~~ (ExprId, StmtId, PatternId, MatchArmId already exist)
- [x] MatchArmId arena already exists in `ExprBody`
- [N/A] TypeRefId approach deferred - using simpler span extraction instead

---

## Phase 2: Create Source Maps

### 2.1 Define HIR Source Map

```rust
// In baml_compiler_hir/src/source_map.rs (new file)
use rowan::ast::SyntaxNodePtr;
use la_arena::ArenaMap;
use rustc_hash::FxHashMap;

pub struct HirSourceMap {
    // Expression spans
    expr_map: ArenaMap<ExprId, SyntaxNodePtr>,
    expr_map_back: FxHashMap<SyntaxNodePtr, ExprId>,

    // Statement spans
    stmt_map: ArenaMap<StmtId, SyntaxNodePtr>,
    stmt_map_back: FxHashMap<SyntaxNodePtr, StmtId>,

    // Pattern spans
    pattern_map: ArenaMap<PatId, SyntaxNodePtr>,
    pattern_map_back: FxHashMap<SyntaxNodePtr, PatId>,

    // Match arm spans
    match_arm_map: ArenaMap<MatchArmId, MatchArmSpans>,
}

pub struct MatchArmSpans {
    pub arm_syntax: SyntaxNodePtr,
    pub pattern_syntax: SyntaxNodePtr,
}

impl HirSourceMap {
    pub fn expr_syntax(&self, id: ExprId) -> Option<SyntaxNodePtr> {
        self.expr_map.get(id).cloned()
    }

    pub fn syntax_expr(&self, ptr: &SyntaxNodePtr) -> Option<ExprId> {
        self.expr_map_back.get(ptr).copied()
    }

    // Similar methods for stmt, pattern, match_arm...
}
```

### 2.2 Define ItemTree Source Map

```rust
pub struct ItemTreeSourceMap {
    // Type reference spans
    type_ref_map: ArenaMap<TypeRefId, SyntaxNodePtr>,
    type_ref_map_back: FxHashMap<SyntaxNodePtr, TypeRefId>,

    // Parameter spans
    param_map: ArenaMap<ParamId, SyntaxNodePtr>,
    param_map_back: FxHashMap<SyntaxNodePtr, ParamId>,
}
```

### 2.3 Combined Query Pattern

```rust
#[salsa::tracked]
pub fn hir_body_with_source_map(
    db: &dyn Db,
    func: Function,
) -> (ExprBody, HirSourceMap) {
    let syntax = db.parse(func.file(db));
    let mut builder = ExprBodyBuilder::new();
    // ... lowering populates both ExprBody and HirSourceMap ...
    (builder.body, builder.source_map)
}

// Convenience: ExprBody is the cached, position-independent part
#[salsa::tracked]
pub fn hir_body(db: &dyn Db, func: Function) -> ExprBody {
    hir_body_with_source_map(db, func).0
}

// SourceMap is recomputed when needed (not cached for incrementality)
pub fn hir_source_map(db: &dyn Db, func: Function) -> HirSourceMap {
    hir_body_with_source_map(db, func).1
}
```

**Tasks:**
- [x] Create `baml_compiler_hir/src/source_map.rs` with `HirSourceMap`
- [x] Create `SignatureSourceMap` for function signature spans
- [x] Implement source map in `FunctionBody::Expr(body, source_map)` tuple pattern
- [x] Add convenience accessors (`expr_span`, `stmt_span`, `pattern_span`, `match_arm_spans`)

---

## Phase 3: Migrate HIR Lowering

### 3.1 Update ExprBody Construction

Remove span HashMaps from `ExprBody` and build source map alongside:

**Before (`body.rs`):**
```rust
pub struct ExprBody {
    pub exprs: Arena<Expr>,
    pub stmts: Arena<Stmt>,
    pub patterns: Arena<Pattern>,
    pub root_expr: Option<ExprId>,
    pub expr_spans: HashMap<ExprId, Span>,      // REMOVE
    pub stmt_spans: HashMap<StmtId, Span>,      // REMOVE
    pub pattern_spans: HashMap<PatId, Span>,    // REMOVE
    pub match_arm_spans: HashMap<ExprId, Vec<MatchArmSpans>>,  // REMOVE
    pub diagnostics: Vec<HirDiagnostic>,
}
```

**After:**
```rust
pub struct ExprBody {
    pub exprs: Arena<Expr>,
    pub stmts: Arena<Stmt>,
    pub patterns: Arena<Pattern>,
    pub match_arms: Arena<MatchArm>,
    pub root_expr: Option<ExprId>,
    pub diagnostics: Vec<HirDiagnostic>,
    // Spans moved to HirSourceMap
}
```

### 3.2 Update Lowering Builder

```rust
struct ExprBodyBuilder {
    body: ExprBody,
    source_map: HirSourceMap,
}

impl ExprBodyBuilder {
    fn alloc_expr(&mut self, expr: Expr, syntax: &SyntaxNode) -> ExprId {
        let id = self.body.exprs.alloc(expr);
        let ptr = SyntaxNodePtr::new(syntax);
        self.source_map.expr_map.insert(id, ptr.clone());
        self.source_map.expr_map_back.insert(ptr, id);
        id
    }

    fn alloc_stmt(&mut self, stmt: Stmt, syntax: &SyntaxNode) -> StmtId {
        let id = self.body.stmts.alloc(stmt);
        let ptr = SyntaxNodePtr::new(syntax);
        self.source_map.stmt_map.insert(id, ptr.clone());
        self.source_map.stmt_map_back.insert(ptr, id);
        id
    }

    // Similar for patterns, match_arms...
}
```

### 3.3 Remove Span Accessor Methods

Delete these methods from `ExprBody`:
- `get_expr_span(&self, expr_id: ExprId) -> Option<Span>`
- `get_stmt_span(&self, stmt_id: StmtId) -> Option<Span>`
- `get_pattern_span(&self, pat_id: PatId) -> Option<Span>`
- `get_match_arm_spans(&self, match_expr_id: ExprId) -> Option<&[MatchArmSpans]>`

**Tasks:**
- [x] Remove `expr_spans`, `stmt_spans`, `pattern_spans`, `match_arm_spans` from `ExprBody`
- [x] `match_arms: Arena<MatchArm>` already existed in `ExprBody`
- [x] Update `ExprBodyBuilder` to build source map alongside body
- [x] Delete span accessor methods from `ExprBody` (moved to `HirSourceMap`)
- [x] Update all call sites to use `HirSourceMap` via `TypeContext.expr_span()` etc.

---

## Phase 4: Migrate FunctionSignature

### 4.1 Update Signature Structure

**Before (`signature.rs`):**
```rust
pub struct FunctionSignature {
    pub name: Name,
    pub params: Vec<Param>,
    pub return_type: TypeRef,
    pub return_type_span: Option<TextRange>,  // REMOVE
}

pub struct Param {
    pub name: Name,
    pub type_ref: TypeRef,
    pub span: Option<TextRange>,  // REMOVE
}
```

**After:**
```rust
pub struct FunctionSignature {
    pub name: Name,
    pub params: Vec<ParamId>,  // Now indices
    pub return_type: TypeRefId,
}

pub struct Param {
    pub name: Name,
    pub type_ref: TypeRefId,
}
```

**Tasks:**
- [x] Remove `return_type_span` field from `FunctionSignature` (moved to `SignatureSourceMap`)
- [x] Remove `span` field from `Param` (moved to `SignatureSourceMap.param_spans`)
- [x] Update `function_signature()` to return `(Arc<FunctionSignature>, SignatureSourceMap)` tuple
- [x] Update all call sites across `baml_ide`, `baml_compiler_emit`, `baml_project`, `tools_onionskin`, `baml_tests`
- [N/A] TypeRefId/ParamId approach deferred - using simpler span extraction

---

## Phase 5: Migrate TIR Type Checking

### 5.1 Update TypeLoweringContext

**Before (`lower.rs`):**
```rust
pub struct TypeLoweringContext<'a> {
    pub known_types: Option<&'a HashSet<Name>>,
    pub span: Option<Span>,  // REMOVE
    pub errors: Vec<TypeError<Ty>>,
}
```

**After:**
```rust
pub struct TypeLoweringContext<'a> {
    pub known_types: Option<&'a HashSet<Name>>,
    pub type_ref_id: Option<TypeRefId>,  // ID for error reporting
    pub file: FileId,
    pub errors: Vec<TypeError<Ty>>,
}
```

### 5.2 Update TypeContext

**Before (`lib.rs`):**
```rust
pub struct TypeContext<'db> {
    // ...
    return_types: Vec<(Ty, Span)>,  // CHANGE
    // ...
}
```

**After:**
```rust
pub struct TypeContext<'db> {
    // ...
    return_types: Vec<(Ty, ExprId)>,  // Now uses ExprId
    // ...
}
```

**Tasks:**
- [x] ~~Replace `Vec<(Ty, Span)>` with `Vec<(Ty, ExprId)>`~~ Simplified to just `Vec<Ty>` since span was unused
- [ ] Replace `span: Option<Span>` with `type_ref_id: Option<TypeRefId>` in `TypeLoweringContext` (deferred to Phase 6)
- [ ] Update all type lowering code to pass IDs instead of spans (requires Phase 6)

**Note:** The `TypeLoweringContext.span` change is intertwined with Phase 6 (changing `TypeError` to use IDs).

---

## Phase 6: Migrate Diagnostics

### 6.1 Update TypeError

**Before (`type_error.rs`):**
```rust
pub enum TypeError<T> {
    TypeMismatch {
        expected: T,
        found: T,
        span: Span,
        info_span: Option<Span>,
    },
    UnknownType { name: String, span: Span },
    // ... all variants have span: Span
}
```

**After:**
```rust
pub enum TypeError<T> {
    TypeMismatch {
        expected: T,
        found: T,
        expr_id: ExprId,
        file: FileId,
        info_source: Option<TypeSource>,
    },
    UnknownType {
        name: String,
        type_ref_id: TypeRefId,
        file: FileId,
    },
    // ... variants use appropriate ID types
}

/// Source of a type (for "expected X because of Y" messages)
pub enum TypeSource {
    TypeRef(TypeRefId, FileId),
    Expr(ExprId, FileId),
    Return(ExprId, FileId),
}
```

### 6.2 Update HirDiagnostic

**Before (`hir_diagnostic.rs`):**
```rust
pub enum HirDiagnostic {
    DuplicateField {
        class_name: String,
        field_name: String,
        first_span: Span,
        second_span: Span,
    },
    // ...
}
```

**After:**
```rust
pub enum HirDiagnostic {
    DuplicateField {
        class_name: String,
        field_name: String,
        first_field_id: FieldId,
        second_field_id: FieldId,
        file: FileId,
    },
    // ...
}
```

### 6.3 Add Diagnostic Rendering

```rust
impl TypeError<Ty> {
    pub fn render(&self, db: &dyn Db) -> RenderedDiagnostic {
        match self {
            TypeError::UnknownType { name, type_ref_id, file } => {
                let source_map = item_tree_source_map(db, *file);
                let span = source_map
                    .type_ref_syntax(*type_ref_id)
                    .map(|ptr| ptr.text_range());

                RenderedDiagnostic {
                    message: format!("Unknown type: {}", name),
                    span,
                    file: *file,
                }
            }
            // ... other variants
        }
    }
}
```

**Tasks:**
- [ ] Define `TypeSource` enum for tracking type origins
- [ ] Update all `TypeError` variants to use IDs instead of spans
- [ ] Update all `HirDiagnostic` variants to use IDs instead of spans
- [ ] Implement `render()` methods for all diagnostic types
- [ ] Update diagnostic emission sites throughout the codebase

---

## Phase 7: Migrate VIR and MIR

### 7.1 Update VIR ExprBody

**Before (`expr.rs`):**
```rust
pub struct ExprBody {
    pub exprs: Arena<Expr>,
    pub patterns: Arena<Pattern>,
    pub expr_types: FxHashMap<ExprId, Ty>,
    pub expr_spans: FxHashMap<ExprId, TextRange>,  // REMOVE
    pub enum_variant_exprs: FxHashMap<ExprId, (Name, Name)>,
    pub root: ExprId,
}
```

**After:**
```rust
pub struct ExprBody {
    pub exprs: Arena<Expr>,
    pub patterns: Arena<Pattern>,
    pub expr_types: FxHashMap<ExprId, Ty>,
    pub enum_variant_exprs: FxHashMap<ExprId, (Name, Name)>,
    pub root: ExprId,
    // Spans obtained via HirSourceMap when needed
}
```

### 7.2 Update VIR Lowering

Remove span extraction from VIR lowering. Instead, VIR can look up spans via HIR source map when rendering errors.

### 7.3 Update MIR

For MIR, spans are useful for debugging but not critical for incrementality (MIR is late in the pipeline). Options:

1. **Keep spans in MIR** - They don't affect caching at this stage
2. **Convert to IDs** - More consistent, allows MIR to remain stable across whitespace changes

Recommended: Keep `span: Option<TextRange>` in MIR for now, but convert to IDs if MIR stability becomes important.

**Tasks:**
- [ ] Remove `expr_spans` from VIR `ExprBody`
- [ ] Remove `span()` method from VIR `ExprBody`
- [ ] Update VIR lowering to not extract spans
- [ ] Update VIR error handling to look up spans via HIR source map
- [ ] Decide on MIR span strategy (keep vs convert)

---

## Phase 8: Update Call Sites

This phase involves updating all code that currently accesses spans directly.

### 8.1 Find All Span Access Sites

Search for:
- `get_expr_span`
- `get_stmt_span`
- `get_pattern_span`
- `get_match_arm_spans`
- `.span` field access on HIR/TIR structures
- `expr_spans.get`
- `stmt_spans.get`

### 8.2 Update Each Site

For each span access:
1. Determine if it's for error reporting or something else
2. If error reporting: pass the ID to the diagnostic, resolve span at render time
3. If IDE feature: use source map lookup

**Tasks:**
- [ ] Audit all span access sites (use grep for patterns above)
- [ ] Update error emission to pass IDs
- [ ] Update IDE features to use source map lookups
- [ ] Ensure no direct span access remains in cached structures

---

## Testing Strategy

### Unit Tests
- [ ] Verify ID stability across whitespace changes
- [ ] Verify source map correctly maps IDs to syntax
- [ ] Verify diagnostics render with correct spans

### Integration Tests
- [ ] Add whitespace to a file → type checking should be cached
- [ ] Change a type name → type checking should re-run
- [ ] Error messages should point to correct locations

### Incrementality Tests
- [ ] Use Salsa's debugging to verify cache hits/misses
- [ ] Benchmark before/after on large files

---

## Migration Strategy

1. **Phase 1-2**: Add ID types and source maps (additive, no behavior change)
2. **Phase 3**: Migrate HIR `ExprBody` (big change, but contained)
3. **Phase 4**: Migrate `FunctionSignature`
4. **Phase 5**: Migrate TIR type checking
5. **Phase 6**: Migrate diagnostics (can be done incrementally per diagnostic type)
6. **Phase 7**: Migrate VIR/MIR
7. **Phase 8**: Clean up remaining call sites

Each phase can be merged independently. The system works correctly throughout migration.

---

## Files to Modify

### New Files
- `baml_compiler_hir/src/source_map.rs` - HirSourceMap
- `baml_base/src/ids.rs` or similar - ID type definitions (if not in existing file)

### Modified Files

| Crate | File | Changes |
|-------|------|---------|
| `baml_base` | `core_types.rs` | Add ID types if needed |
| `baml_compiler_hir` | `body.rs` | Remove span HashMaps, add match_arms arena |
| `baml_compiler_hir` | `signature.rs` | Remove span fields, use IDs |
| `baml_compiler_hir` | `lower.rs` (or equivalent) | Build source map during lowering |
| `baml_compiler_tir` | `lower.rs` | Replace span with TypeRefId |
| `baml_compiler_tir` | `lib.rs` | Replace (Ty, Span) with (Ty, ExprId) |
| `baml_compiler_diagnostics` | `type_error.rs` | Replace spans with IDs |
| `baml_compiler_diagnostics` | `hir_diagnostic.rs` | Replace spans with IDs |
| `baml_compiler_vir` | `expr.rs` | Remove expr_spans |
| `baml_compiler_vir` | `lower.rs` | Remove span extraction |
| `baml_compiler_mir` | `ir.rs` | Optionally convert spans to IDs |

---

## Open Questions

1. **Granularity**: Should we track source for every sub-expression in complex types like `map<string, int>`, or just the top-level type?

2. **Generic parameters**: How to handle source locations for inferred type parameters?

3. **Cross-file tracking**: For types imported from other files, do we need to track both the import site and the definition site?

4. **MIR spans**: Keep for debugging convenience, or convert to IDs for consistency?

5. **MatchArmId vs ExprId**: Should match arms be in their own arena, or continue to be keyed by the match expression's ExprId?
