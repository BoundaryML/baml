# BAML THIR Type Checker Implementation Plan

**Status**: In Progress  
**Date**: 2025-11-24  
**Goal**: Build a static bidirectional type checker for BAML using Salsa, similar to rust-analyzer and ruff

---

## Executive Summary

This document outlines the implementation plan for `baml_thir`, a typed high-level IR layer that performs bidirectional type inference for BAML. The design follows patterns from rust-analyzer (Salsa-based incrementality, interned types) and ruff (bidirectional typing with type context propagation).

### Current Status

**Implemented:**
- ✅ Basic type representation (`Ty` enum in `ty.rs`)
- ✅ Type context with scope management (`TypeContext` in `inference.rs`)
- ✅ Expression and statement inference skeleton (`check.rs`)
- ✅ Subtyping rules with union types
- ✅ Database trait structure (`Db` extends `baml_hir::Db`)
- ✅ Main query entry point (`typecheck_function`)

**Missing:**
- ❌ Connection to HIR bodies (can't access `ExprBody` from `FunctionId`)
- ❌ TyLowering layer (convert `TypeRef` → `Ty`)
- ❌ Function signature queries
- ❌ Proper diagnostic representation with spans
- ❌ Type interning for compound types
- ❌ Name resolution for type paths
- ❌ Database implementation in `baml_db`

---

## Architecture Overview

### Layered Design

```
┌─────────────────────────────────────────────────────────────┐
│ IDE / CLI (baml_db)                                         │
│ - RootDatabase impl                                         │
│ - Orchestrates all queries                                  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ THIR (baml_thir) - TYPE CHECKING LAYER                      │
│                                                              │
│ Queries:                                                     │
│  • typecheck_function(FunctionId) → TypeCheckResult         │
│  • lower_type_ref(TypeRef) → Ty                             │
│  • function_signature_ty(FunctionId) → FunctionSignature    │
│                                                              │
│ Non-Query Helpers:                                           │
│  • infer_function_body() - main inference logic             │
│  • check_expr() / infer_expr() - bidirectional typing       │
│  • TypeContext - accumulates results and diagnostics        │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ HIR (baml_hir) - SYNTAX-INDEPENDENT IR                      │
│                                                              │
│ Queries:                                                     │
│  • file_item_tree(SourceFile) → ItemTree                    │
│  • function_signature(FunctionId) → FunctionSignature       │
│  • function_body(FunctionId) → FunctionBody                 │
│                                                              │
│ Data Structures:                                             │
│  • ItemTree - position-independent items                    │
│  • ExprBody - arena-based expression IR                     │
│  • TypeRef - syntactic type references                      │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Parser (baml_parser)                                        │
│ - CST → syntax tree                                         │
└─────────────────────────────────────────────────────────────┘
```

### Key Principles

1. **Salsa Queries for Incrementality**: Only recompute what changed
2. **Bidirectional Type Checking**: Push types down (check), synthesize up (infer)
3. **Arena-Based IRs**: Use indices instead of pointers
4. **Interned Types**: Cheap comparison, handles cycles
5. **Diagnostic Accumulation**: Errors don't abort, just accumulate

---

## Phase 1: Foundation (PRIORITY: HIGH)

### 1.1 Fix HIR Body Access

**Problem**: `typecheck_function` can't access function bodies

**Current Issue** (`check.rs:10-28`):
```rust
pub fn infer_function_body<'db>(db: &'db dyn Db, func: FunctionId<'db>) -> InferenceResult {
    let mut ctx = TypeContext::new(db, func);
    let item_tree = baml_hir::file_item_tree(db, func.file(db));
    let _func_def = &item_tree[func.id(db)];
    
    // TODO: We need a way to get the body from the function ID.
    // Currently baml_hir separates item tree (signatures) from bodies.
    // ...
}
```

**Solution**: HIR already has the infrastructure, just need to wire it up

**Action Items**:

1. **Verify `baml_db` has the body query** (should be at `baml_db/src/lib.rs:131-155`):
   ```rust
   #[salsa::tracked]
   pub fn function_body<'db>(
       db: &'db dyn baml_hir::Db,
       file: SourceFile,
       function: baml_hir::FunctionLoc<'db>,
   ) -> Arc<baml_hir::FunctionBody>
   ```

2. **Add helper to `FunctionId` to get body** (in `baml_hir/src/lib.rs`):
   ```rust
   impl<'db> FunctionId<'db> {
       pub fn body(self, db: &'db dyn Db) -> Option<Arc<FunctionBody>> {
           let file = self.file(db);
           let body = function_body(db, file, self);
           match &*body {
               FunctionBody::Expr(expr_body) => Some(body),
               _ => None, // LLM or Missing bodies don't have expr bodies
           }
       }
   }
   ```

3. **Update `infer_function_body` to use it**:
   ```rust
   pub fn infer_function_body<'db>(db: &'db dyn Db, func: FunctionId<'db>) -> InferenceResult {
       let mut ctx = TypeContext::new(db, func);
       
       let body = match func.body(db) {
           Some(body) => body,
           None => {
               // LLM function or missing body - no type checking needed
               return InferenceResult {
                   expr_types: HashMap::new(),
                   diagnostics: Vec::new(),
               };
           }
       };
       
       let expr_body = match &*body {
           FunctionBody::Expr(expr_body) => expr_body,
           _ => unreachable!(),
       };
       
       // Now we can typecheck!
       if let Some(root_expr) = expr_body.root_expr {
           infer_expr(&mut ctx, root_expr, expr_body);
       }
       
       InferenceResult {
           expr_types: ctx.expr_types,
           diagnostics: ctx.diagnostics,
       }
   }
   ```

**Verification**: Write a test that calls `typecheck_function` on a simple function

**Files to Modify**:
- `baml_hir/src/lib.rs` - Add helper method
- `baml_thir/src/check.rs` - Update `infer_function_body`

---

### 1.2 Implement TyLowering

**Problem**: Can't convert HIR's `TypeRef` to THIR's `Ty`

**Current Gap**: HIR uses `TypeRef` (syntactic), THIR needs `Ty` (semantic)

**Solution**: Add a lowering layer that resolves names

**Action Items**:

1. **Create `baml_thir/src/lower.rs`**:
   ```rust
   //! Lower HIR TypeRef to THIR Ty.
   
   use crate::{Db, Ty};
   use baml_hir::TypeRef;
   use baml_base::Name;
   
   pub(crate) struct TyLowering<'db> {
       db: &'db dyn Db,
   }
   
   impl<'db> TyLowering<'db> {
       pub fn new(db: &'db dyn Db) -> Self {
           Self { db }
       }
       
       pub fn lower(&mut self, type_ref: &TypeRef) -> Ty {
           match type_ref {
               // Primitives
               TypeRef::Int => Ty::Int,
               TypeRef::Float => Ty::Float,
               TypeRef::String => Ty::String,
               TypeRef::Bool => Ty::Bool,
               TypeRef::Null => Ty::Null,
               TypeRef::Image => Ty::Image,
               TypeRef::Audio => Ty::Audio,
               TypeRef::Video => Ty::Video,
               TypeRef::Pdf => Ty::Pdf,
               
               // Compound types
               TypeRef::List(inner) => {
                   let inner_ty = self.lower(inner);
                   Ty::List(Box::new(inner_ty))
               }
               
               TypeRef::Map(k, v) => {
                   let k_ty = self.lower(k);
                   let v_ty = self.lower(v);
                   Ty::Map(Box::new(k_ty), Box::new(v_ty))
               }
               
               TypeRef::Optional(inner) => {
                   let inner_ty = self.lower(inner);
                   Ty::Union(vec![inner_ty, Ty::Null])
               }
               
               TypeRef::Union(types) => {
                   let tys = types.iter().map(|t| self.lower(t)).collect();
                   Ty::Union(tys)
               }
               
               // Named types - need resolution
               TypeRef::Named(name) => {
                   self.resolve_named_type(name)
               }
               
               TypeRef::Unknown => Ty::Unknown,
           }
       }
       
       fn resolve_named_type(&self, name: &Name) -> Ty {
           // TODO: Implement name resolution
           // For now, return Unknown
           // Future: Look up in project items, find ClassId/EnumId
           Ty::Unknown
       }
   }
   ```

2. **Add query for lowering** (in `baml_thir/src/lib.rs`):
   ```rust
   /// Lower a TypeRef to a Ty.
   #[salsa::tracked]
   pub fn lower_type_ref<'db>(db: &'db dyn Db, type_ref: TypeRef) -> Ty {
       let mut lowering = crate::lower::TyLowering::new(db);
       lowering.lower(&type_ref)
   }
   ```

3. **Use in function signature lowering**:
   ```rust
   #[salsa::tracked]
   pub fn function_signature_ty<'db>(
       db: &'db dyn Db,
       func: baml_hir::FunctionId<'db>,
   ) -> FunctionSignatureTy {
       let sig = baml_hir::function_signature(db, func.file(db), func);
       
       FunctionSignatureTy {
           params: sig.params.iter().map(|p| {
               let ty = lower_type_ref(db, p.type_ref.clone());
               (p.name.clone(), ty)
           }).collect(),
           return_type: sig.return_type
               .as_ref()
               .map(|t| lower_type_ref(db, t.clone()))
               .unwrap_or(Ty::Void),
       }
   }
   ```

**Files to Create/Modify**:
- `baml_thir/src/lower.rs` (new file)
- `baml_thir/src/lib.rs` - Add module and query

---

### 1.3 Proper Diagnostic Representation

**Problem**: Diagnostics are just `Vec<String>`, need spans

**Solution**: Use structured diagnostics like rust-analyzer

**Action Items**:

1. **Create `baml_thir/src/diagnostic.rs`**:
   ```rust
   //! Type checking diagnostics.
   
   use baml_base::{Span, Diagnostic as BaseDiagnostic, Severity};
   use baml_hir::ExprId;
   use crate::Ty;
   
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct TypeCheckDiagnostic {
       pub kind: DiagnosticKind,
       pub expr: ExprId,
   }
   
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum DiagnosticKind {
       TypeMismatch { expected: Ty, found: Ty },
       UnknownVariable { name: String },
       InvalidBinaryOp { op: String, lhs: Ty, rhs: Ty },
       // Add more as needed
   }
   
   impl TypeCheckDiagnostic {
       pub fn message(&self) -> String {
           match &self.kind {
               DiagnosticKind::TypeMismatch { expected, found } => {
                   format!("Type mismatch: expected {}, found {}", expected, found)
               }
               DiagnosticKind::UnknownVariable { name } => {
                   format!("Unknown variable: {}", name)
               }
               DiagnosticKind::InvalidBinaryOp { op, lhs, rhs } => {
                   format!("Invalid operands for {}: {} and {}", op, lhs, rhs)
               }
           }
       }
   }
   ```

2. **Update `InferenceResult`**:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct InferenceResult {
       pub expr_types: HashMap<ExprId, Ty>,
       pub diagnostics: Vec<TypeCheckDiagnostic>,
   }
   ```

3. **Update `TypeContext` to use structured diagnostics**:
   ```rust
   impl<'db> TypeContext<'db> {
       pub fn push_error(&mut self, expr: ExprId, kind: DiagnosticKind) {
           self.diagnostics.push(TypeCheckDiagnostic { kind, expr });
       }
   }
   ```

**Files to Create/Modify**:
- `baml_thir/src/diagnostic.rs` (new file)
- `baml_thir/src/inference.rs` - Update result type
- `baml_thir/src/check.rs` - Use structured diagnostics

---

### 1.4 Database Implementation

**Problem**: `baml_db` doesn't implement `baml_thir::Db`

**Solution**: Add the trait impl

**Action Items**:

1. **Update `baml_db/src/lib.rs`** (add after line 40):
   ```rust
   #[salsa::db]
   impl baml_thir::Db for RootDatabase {}
   ```

2. **Add dependency in `baml_db/Cargo.toml`**:
   ```toml
   [dependencies]
   baml_thir = { path = "../baml_thir" }
   ```

**Files to Modify**:
- `baml_db/src/lib.rs`
- `baml_db/Cargo.toml`

---

## Phase 2: Core Type Checking (PRIORITY: HIGH)

### 2.1 Complete Expression Inference

**Goal**: Implement all expression types in `infer_expr`

**Current Coverage** (from `check.rs:45-184`):
- ✅ Literals
- ✅ Paths (variables)
- ✅ Binary operations
- ✅ Unary operations
- ✅ Blocks
- ✅ If expressions
- ✅ Arrays
- ❌ Calls (partial - no signature lookup)
- ❌ Objects (missing)
- ❌ Field access (missing)
- ❌ Index access (missing)

**Action Items**:

1. **Implement function call inference**:
   ```rust
   Expr::Call { callee, args } => {
       // Infer callee type
       let callee_ty = infer_expr(ctx, *callee, body);
       
       // Get function signature
       match callee_ty {
           Ty::Function(sig) => {
               // Check argument count
               if args.len() != sig.params.len() {
                   ctx.push_error(expr_id, DiagnosticKind::ArgumentCountMismatch {
                       expected: sig.params.len(),
                       found: args.len(),
                   });
               }
               
               // Check argument types
               for (arg_expr, param_ty) in args.iter().zip(sig.params.iter()) {
                   check_expr(ctx, *arg_expr, param_ty, body);
               }
               
               sig.return_type
           }
           _ => {
               ctx.push_error(expr_id, DiagnosticKind::CallNonCallable { ty: callee_ty });
               Ty::Unknown
           }
       }
   }
   ```

2. **Implement object literal inference**:
   ```rust
   Expr::Object { type_name, fields } => {
       // Look up class type if name provided
       let class_ty = if let Some(name) = type_name {
           // TODO: Resolve name to ClassId
           // For now, Unknown
           Ty::Unknown
       } else {
           // Anonymous object - infer structural type
           Ty::Unknown
       };
       
       // Infer field types
       for (field_name, field_expr) in fields {
           infer_expr(ctx, *field_expr, body);
       }
       
       class_ty
   }
   ```

3. **Implement field access**:
   ```rust
   Expr::FieldAccess { base, field } => {
       let base_ty = infer_expr(ctx, *base, body);
       
       match base_ty {
           Ty::Class(class_id) => {
               // TODO: Look up field in class
               // For now, Unknown
               Ty::Unknown
           }
           _ => {
               ctx.push_error(expr_id, DiagnosticKind::FieldAccessOnNonClass { ty: base_ty });
               Ty::Unknown
           }
       }
   }
   ```

4. **Implement index access**:
   ```rust
   Expr::Index { base, index } => {
       let base_ty = infer_expr(ctx, *base, body);
       let index_ty = infer_expr(ctx, *index, body);
       
       match base_ty {
           Ty::List(elem_ty) => {
               // Check index is int
               check_expr(ctx, *index, &Ty::Int, body);
               *elem_ty
           }
           Ty::Map(key_ty, val_ty) => {
               // Check index matches key type
               check_expr(ctx, *index, &*key_ty, body);
               *val_ty
           }
           _ => {
               ctx.push_error(expr_id, DiagnosticKind::IndexOnNonIndexable { ty: base_ty });
               Ty::Unknown
           }
       }
   }
   ```

**Files to Modify**:
- `baml_thir/src/check.rs`

---

### 2.2 Statement Type Checking

**Goal**: Complete statement checking logic

**Action Items**:

1. **Improve let statement handling**:
   ```rust
   Stmt::Let { pattern, type_annotation, initializer } => {
       // Infer or check initializer
       let init_ty = if let Some(annot) = type_annotation {
           let annot_ty = lower_type_ref(ctx.db, annot.clone());
           if let Some(init) = initializer {
               check_expr(ctx, *init, &annot_ty, body);
           }
           annot_ty
       } else if let Some(init) = initializer {
           infer_expr(ctx, *init, body)
       } else {
           Ty::Unknown
       };
       
       // Bind pattern
       match &body.patterns[*pattern] {
           Pattern::Binding(name) => {
               ctx.define(name.clone(), init_ty);
           }
       }
   }
   ```

2. **Add return type checking**:
   ```rust
   Stmt::Return(expr) => {
       let return_ty = if let Some(e) = expr {
           infer_expr(ctx, *e, body)
       } else {
           Ty::Void
       };
       
       // TODO: Check against function return type
       // For now, just infer
   }
   ```

**Files to Modify**:
- `baml_thir/src/check.rs` - Update `check_stmt`

---

### 2.3 Function Signature Integration

**Goal**: Check function body against declared signature

**Action Items**:

1. **Add return type checking** (in `infer_function_body`):
   ```rust
   pub fn infer_function_body<'db>(db: &'db dyn Db, func: FunctionId<'db>) -> InferenceResult {
       let mut ctx = TypeContext::new(db, func);
       
       // Get function signature
       let sig = function_signature_ty(db, func);
       
       // Add parameters to scope
       for (param_name, param_ty) in &sig.params {
           ctx.define(param_name.clone(), param_ty.clone());
       }
       
       // Get and check body
       let body = match func.body(db) {
           Some(body) => body,
           None => return InferenceResult::empty(),
       };
       
       let expr_body = match &*body {
           FunctionBody::Expr(expr_body) => expr_body,
           _ => return InferenceResult::empty(),
       };
       
       // Infer root expression and check return type
       if let Some(root_expr) = expr_body.root_expr {
           let body_ty = infer_expr(&mut ctx, root_expr, expr_body);
           
           if !body_ty.is_subtype_of(&sig.return_type, db) {
               ctx.push_error(
                   root_expr,
                   DiagnosticKind::TypeMismatch {
                       expected: sig.return_type,
                       found: body_ty,
                   },
               );
           }
       }
       
       InferenceResult {
           expr_types: ctx.expr_types,
           diagnostics: ctx.diagnostics,
       }
   }
   ```

**Files to Modify**:
- `baml_thir/src/check.rs`

---

## Phase 3: Type System Enhancements (PRIORITY: MEDIUM)

### 3.1 Type Interning

**Goal**: Intern compound types for efficient comparison

**Current Issue**: `Ty::List(Box<Ty>)` is not interned, so comparing large types is expensive

**Solution**: Use Salsa interning

**Action Items**:

1. **Change `Ty` representation** (in `ty.rs`):
   ```rust
   // Instead of:
   #[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update)]
   pub enum Ty { ... }
   
   // Use:
   #[salsa::interned]
   pub struct Ty {
       #[return_ref]
       pub kind: TyKind,
   }
   
   #[derive(Debug, Clone, PartialEq, Eq, Hash)]
   pub enum TyKind {
       Int, Float, String, Bool, Null,
       Class(salsa::Id),
       Enum(salsa::Id),
       List(Ty),  // Now Ty is just an ID
       Map(Ty, Ty),
       Union(Vec<Ty>),
       Unknown, Void,
       Image, Audio, Video, Pdf,
   }
   ```

2. **Add convenience constructors**:
   ```rust
   impl Ty {
       pub fn int(db: &dyn Db) -> Ty {
           Ty::new(db, TyKind::Int)
       }
       
       pub fn list(db: &dyn Db, elem: Ty) -> Ty {
           Ty::new(db, TyKind::List(elem))
       }
       
       pub fn union(db: &dyn Db, types: Vec<Ty>) -> Ty {
           Ty::new(db, TyKind::Union(types))
       }
       
       // etc.
   }
   ```

3. **Update all type checking code** to use constructors and pass `db`

**Trade-off**: This is a significant refactor. Consider deferring until performance becomes an issue.

**Files to Modify**:
- `baml_thir/src/ty.rs`
- All files that construct `Ty` values

---

### 3.2 Union Type Normalization

**Goal**: Simplify union types to canonical form

**Action Items**:

1. **Add union normalization**:
   ```rust
   impl Ty {
       pub fn normalize_union(db: &dyn Db, types: Vec<Ty>) -> Ty {
           let mut normalized = Vec::new();
           
           for ty in types {
               match ty.kind(db) {
                   // Flatten nested unions
                   TyKind::Union(inner) => {
                       normalized.extend(inner.clone());
                   }
                   // Remove duplicates
                   _ => {
                       if !normalized.contains(&ty) {
                           normalized.push(ty);
                       }
                   }
               }
           }
           
           // Simplify
           if normalized.is_empty() {
               Ty::never(db)
           } else if normalized.len() == 1 {
               normalized[0]
           } else {
               Ty::union(db, normalized)
           }
       }
   }
   ```

**Files to Modify**:
- `baml_thir/src/ty.rs`

---

### 3.3 Improved Subtyping

**Goal**: More precise subtyping rules

**Action Items**:

1. **Add variance tracking**:
   - Lists: Covariant in element type
   - Maps: Invariant in key, covariant in value (for immutable maps)

2. **Handle union distribution**:
   - `(A | B)[]` is subtype of `A[] | B[]`

3. **Add literal types**:
   - `Literal[1]` is subtype of `int`
   - Useful for enum variants and boolean narrowing

**Files to Modify**:
- `baml_thir/src/ty.rs` - Extend `is_subtype_of`

---

## Phase 4: Advanced Features (PRIORITY: LOW)

### 4.1 Name Resolution

**Goal**: Resolve type names to ClassId/EnumId

**Action Items**:

1. **Create `baml_thir/src/resolve.rs`**:
   ```rust
   pub struct TypeResolver<'db> {
       db: &'db dyn Db,
       scope: ScopeId,  // Current scope for lookups
   }
   
   impl<'db> TypeResolver<'db> {
       pub fn resolve_type_name(&self, name: &Name) -> Option<Ty> {
           // Look up in project items
           let items = baml_hir::project_items(self.db, /* root */);
           
           for item in items.items(self.db) {
               match item {
                   ItemId::Class(class) => {
                       if class.name(self.db) == *name {
                           return Some(Ty::Class(class.into()));
                       }
                   }
                   ItemId::Enum(enum_) => {
                       if enum_.name(self.db) == *name {
                           return Some(Ty::Enum(enum_.into()));
                       }
                   }
                   _ => {}
               }
           }
           
           None
       }
   }
   ```

2. **Use in TyLowering**:
   ```rust
   fn resolve_named_type(&self, name: &Name) -> Ty {
       let resolver = TypeResolver::new(self.db, /* scope */);
       resolver.resolve_type_name(name)
           .unwrap_or(Ty::Unknown)
   }
   ```

**Files to Create/Modify**:
- `baml_thir/src/resolve.rs` (new)
- `baml_thir/src/lower.rs` - Use resolver

---

### 4.2 Class Field Lookup

**Goal**: Type check field access on classes

**Action Items**:

1. **Add query to get class fields**:
   ```rust
   #[salsa::tracked]
   pub fn class_fields<'db>(
       db: &'db dyn Db,
       class: baml_hir::ClassId<'db>,
   ) -> Arc<Vec<(Name, Ty)>> {
       let item_tree = baml_hir::file_item_tree(db, class.file(db));
       let class_data = &item_tree[class.id(db)];
       
       let fields = class_data.fields.iter().map(|field| {
           let ty = lower_type_ref(db, field.type_ref.clone());
           (field.name.clone(), ty)
       }).collect();
       
       Arc::new(fields)
   }
   ```

2. **Use in field access inference**:
   ```rust
   Expr::FieldAccess { base, field } => {
       let base_ty = infer_expr(ctx, *base, body);
       
       match base_ty {
           Ty::Class(class_id) => {
               let fields = class_fields(ctx.db, class_id);
               
               fields.iter()
                   .find(|(name, _)| name == field)
                   .map(|(_, ty)| ty.clone())
                   .unwrap_or_else(|| {
                       ctx.push_error(expr_id, DiagnosticKind::NoSuchField {
                           class: class_id,
                           field: field.clone(),
                       });
                       Ty::Unknown
                   })
           }
           _ => {
               ctx.push_error(expr_id, DiagnosticKind::FieldAccessOnNonClass { ty: base_ty });
               Ty::Unknown
           }
       }
   }
   ```

**Files to Modify**:
- `baml_thir/src/lib.rs` - Add query
- `baml_thir/src/check.rs` - Use in field access

---

### 4.3 Control Flow Analysis

**Goal**: Type narrowing in conditionals (like ruff's use-def chains)

**Example**:
```baml
function classify(x: int | null) -> string {
    if x != null {
        // Here x is narrowed to `int`
        return x.to_string()
    }
    return "null"
}
```

**Action Items**:

1. **Track narrowing predicates**:
   - `x is not None` → narrow `x` from `T | None` to `T`
   - `typeof(x) == "int"` → narrow `x` to `int`

2. **Implement predicate-based narrowing**:
   ```rust
   pub struct Predicate {
       pub var: Name,
       pub constraint: Constraint,
   }
   
   pub enum Constraint {
       IsNotNull,
       IsType(Ty),
   }
   ```

3. **Use in if expressions**:
   ```rust
   Expr::If { condition, then_branch, else_branch } => {
       // Infer condition
       check_expr(ctx, *condition, &Ty::Bool, body);
       
       // Extract predicates from condition
       let predicates = extract_predicates(ctx, *condition, body);
       
       // Push scope with narrowed types for then branch
       ctx.push_scope();
       for predicate in predicates {
           apply_predicate(ctx, &predicate);
       }
       let then_ty = infer_expr(ctx, *then_branch, body);
       ctx.pop_scope();
       
       // Else branch with inverse predicates
       ctx.push_scope();
       for predicate in predicates {
           apply_inverse_predicate(ctx, &predicate);
       }
       let else_ty = if let Some(else_expr) = else_branch {
           infer_expr(ctx, *else_expr, body)
       } else {
           Ty::Void
       };
       ctx.pop_scope();
       
       // Unify branches
       unify(ctx.db, then_ty, else_ty)
   }
   ```

**Files to Create/Modify**:
- `baml_thir/src/narrowing.rs` (new)
- `baml_thir/src/check.rs` - Use in if expressions

---

## Testing Strategy

### Unit Tests

**Location**: `baml_thir/src/tests/`

**Test Categories**:

1. **Basic type inference**:
   ```rust
   #[test]
   fn test_literal_inference() {
       let db = TestDatabase::new();
       let func = create_function(&db, "function f() -> int { 42 }");
       let result = typecheck_function(&db, func);
       assert_eq!(result.diagnostics.len(), 0);
   }
   ```

2. **Type mismatches**:
   ```rust
   #[test]
   fn test_type_mismatch() {
       let db = TestDatabase::new();
       let func = create_function(&db, "function f() -> int { \"hello\" }");
       let result = typecheck_function(&db, func);
       assert_eq!(result.diagnostics.len(), 1);
       assert!(matches!(
           result.diagnostics[0].kind,
           DiagnosticKind::TypeMismatch { .. }
       ));
   }
   ```

3. **Scope tracking**:
   ```rust
   #[test]
   fn test_variable_shadowing() {
       let db = TestDatabase::new();
       let func = create_function(&db, r#"
           function f() -> int {
               let x = 1;
               {
                   let x = "hello";
                   return x;  // Should be string
               }
           }
       "#);
       let result = typecheck_function(&db, func);
       assert!(result.diagnostics.len() > 0);
   }
   ```

### Integration Tests

**Location**: `baml_db/tests/`

**Test Categories**:

1. **Cross-function calls**
2. **Class field access**
3. **Enum matching**

---

## Current Implementation Review

### What to Keep

1. ✅ **Basic structure** (`lib.rs`, `ty.rs`, `inference.rs`, `check.rs`) - Good separation
2. ✅ **Type representation** - `Ty` enum covers BAML's type system well
3. ✅ **Bidirectional checking** - `check_expr` vs `infer_expr` is the right pattern
4. ✅ **Scope management** - `push_scope`/`pop_scope` with variable lookup works
5. ✅ **Subtyping rules** - Basic implementation is correct, can be extended
6. ✅ **Query structure** - `typecheck_function` as main entry point is good

### What to Discard/Refactor

1. ❌ **String diagnostics** → Replace with structured `TypeCheckDiagnostic`
2. ❌ **Missing body access** → Wire up HIR body queries
3. ❌ **No TyLowering** → Add `lower.rs` module
4. ❌ **Incomplete expression handling** → Implement missing cases
5. ⚠️ **Type representation** → Consider interning (but not urgent)

### What's Missing

1. **TyLowering layer** - Convert `TypeRef` → `Ty`
2. **Name resolution** - Resolve class/enum names
3. **Function signatures** - Query to get typed signatures
4. **Field lookup** - Query to get class fields
5. **Database impl** - Add `impl baml_thir::Db for RootDatabase`

---

## Implementation Roadmap

### Week 1: Foundation
- [ ] Fix HIR body access (1.1)
- [ ] Implement TyLowering (1.2)
- [ ] Structured diagnostics (1.3)
- [ ] Database impl (1.4)
- [ ] Write basic tests

### Week 2: Core Features
- [ ] Complete expression inference (2.1)
- [ ] Improve statement checking (2.2)
- [ ] Function signature integration (2.3)
- [ ] Write integration tests

### Week 3: Type System
- [ ] Union normalization (3.2)
- [ ] Improved subtyping (3.3)
- [ ] More comprehensive tests

### Week 4: Advanced Features (Optional)
- [ ] Name resolution (4.1)
- [ ] Class field lookup (4.2)
- [ ] Type interning (3.1) if needed
- [ ] Control flow analysis (4.3) if time permits

---

## Salsa Usage Patterns

### Queries to Add

1. **`typecheck_function(FunctionId) → TypeCheckResult`** - Already exists ✅
2. **`lower_type_ref(TypeRef) → Ty`** - Need to add
3. **`function_signature_ty(FunctionId) → FunctionSignatureTy`** - Need to add
4. **`class_fields(ClassId) → Vec<(Name, Ty)>`** - Need to add
5. **`resolve_type_name(Name) → Option<Ty>`** - Need to add (maybe)

### Non-Query Helpers

Keep these as regular functions (not tracked):
- `infer_function_body` - Internal to typecheck_function
- `infer_expr` / `check_expr` - Internal helpers
- `TypeContext` methods - State management

### Dependency Graph

```
typecheck_function
    ↓
function_signature_ty
    ↓
lower_type_ref
    ↓
resolve_type_name (maybe)

typecheck_function
    ↓
function_body (from HIR)
    ↓
file_item_tree (from HIR)

class_fields
    ↓
lower_type_ref
```

---

## Open Questions & Decisions Needed

### 1. Type Interning - When?

**Question**: Should we intern all `Ty` values now or later?

**Options**:
- **Option A**: Intern now (Phase 3.1) - More work upfront, better performance
- **Option B**: Defer until performance matters - Simpler, iterate faster

**Recommendation**: Option B. Use `#[derive(Clone)]` for now, intern later if needed.

---

### 2. Name Resolution - Scope?

**Question**: Where does name resolution belong?

**Options**:
- **Option A**: In THIR (this layer) - Simpler, but THIR depends on project structure
- **Option B**: In HIR as a separate "name resolution" module - More principled
- **Option C**: In a new `baml_resolve` crate - Maximum modularity

**Recommendation**: Option A for now (simpler), refactor to B later if needed.

---

### 3. Control Flow - Essential?

**Question**: Is control-flow-sensitive type narrowing essential for BAML?

**Context**: Ruff needs it for Python (dynamic typing), but BAML is more static.

**Recommendation**: Defer to Phase 4. Focus on getting basic type checking working first.

---

### 4. LLM Functions - Type Check?

**Question**: Should we type-check LLM function prompts?

**Context**: LLM functions have `{{ variable }}` interpolations

**Options**:
- **Option A**: Type check interpolations (ensure variables exist, types match)
- **Option B**: Skip LLM functions entirely
- **Option C**: Check interpolations but not prompt structure

**Recommendation**: Option C. Check that interpolated variables exist and have types, but don't validate prompt semantics.

---

## References

### rust-analyzer Architecture
- **File**: `/Users/greghale/code/rust-analyzer/crates/hir-ty/src/`
- **Key Patterns**: Salsa queries, type interning, TyLowering, InferenceContext

### ruff Type Checker
- **File**: `/Users/greghale/code/ruff/crates/ty_python_semantic/src/`
- **Key Patterns**: Three-tier inference (scope/def/expr), use-def chains, constraint-based generics

### BAML Current Implementation
- **HIR**: `baml_language/crates/baml_hir/`
- **THIR**: `baml_language/crates/baml_thir/`
- **Database**: `baml_language/crates/baml_db/`

---

## Conclusion

This implementation plan provides a roadmap for completing the BAML type checker using Salsa and bidirectional typing. The architecture follows proven patterns from rust-analyzer and ruff, adapted to BAML's domain-specific needs.

**Key Success Metrics**:
1. Type check all expression functions correctly
2. Report precise diagnostics with spans
3. Incremental recomputation via Salsa
4. Pass comprehensive test suite

**Next Steps**:
1. Start with Phase 1 (Foundation) to unblock the rest
2. Iterate on Phase 2 (Core Type Checking) with tests
3. Evaluate Phases 3-4 based on project priorities
