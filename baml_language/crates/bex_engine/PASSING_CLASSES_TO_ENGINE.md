# Passing Class Instances to BexEngine from External Code

## Problem Statement

When calling a BAML function from external code (e.g., via the Rust FFI), we need to pass arguments that may include class instances. The current flow is:

```
External Code (Rust/Python/TS)
    ↓
BexExternalValue::Instance { class_name: "MyClass", fields: {...} }
    ↓
allocate_from_external()
    ↓
Object::Instance(Instance { class: HeapPtr, fields: Vec<Value> })
```

The `allocate_from_external` function in `bex_engine/src/lib.rs` currently has a `todo!()` for the `Instance` case because it lacks the information needed to perform this conversion:

```rust
BexExternalValue::Instance { .. } => {
    todo!(
        "Cannot allocate Instance from BexExternalValue. \
         We need to do a string lookup for the right type in the schema."
    )
}
```

### What's Missing

To convert `BexExternalValue::Instance` to `Object::Instance`, we need:

1. **Class Pointer Lookup**: Map `class_name: String` → `HeapPtr` pointing to the `Object::Class` in the VM's heap
2. **Field Ordering**: The VM's `Instance` stores fields as `Vec<Value>` ordered by field index, but `BexExternalValue::Instance` has `IndexMap<String, _>` keyed by field name

Both pieces of information exist in the schema (`BexEngine::snapshot.classes`), but `allocate_from_external` is a static method that only receives `&mut BexVm`.

---

## Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        BexEngine                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ snapshot: BexSnapshot                                 │  │
│  │   └── classes: HashMap<String, ClassDef>             │  │ ◄── Has class metadata
│  │         └── fields: Vec<FieldDef> (ordered)          │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ fn externalize_to_value(vm, external, guard)         │  │
│  │   └── Self::allocate_from_external(vm, ext, guard)   │  │ ◄── Static method, no &self
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                          BexVm                              │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ globals: GlobalPool                                   │  │ ◄── Contains Class objects
│  │ tlab: Tlab                                           │  │     (but no name lookup)
│  │ heap: Arc<BexHeap>                                   │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

The Class objects exist in `vm.globals`, but there's no efficient way to look them up by name from within `allocate_from_external`.

---

## Design Options

### Option 1: Pass Schema Reference as Parameter

**Approach**: Add a class lookup parameter to `allocate_from_external`.

```rust
fn allocate_from_external(
    vm: &mut BexVm,
    external: &BexExternalValue,
    guard: &EpochGuard<'_>,
    class_lookup: &HashMap<String, (HeapPtr, Vec<String>)>,  // name → (ptr, field_order)
) -> Value
```

**Implementation**:
- Build `class_lookup` once when creating the engine from `snapshot.classes` + allocated Class objects
- Thread it through `externalize_to_value` → `allocate_from_external`
- Handle recursive calls (Instance fields may contain nested Instances)

**Pros**:
- Explicit dependency injection
- No changes to BexVm structure
- Clear what information flows where

**Cons**:
- Ripples through call chain (every caller needs the parameter)
- Recursive calls require passing the same parameter
- More verbose function signatures

---

### Option 2: Add Class Lookup Table to BexVm

**Approach**: Store class metadata directly in the VM.

```rust
pub struct BexVm {
    // ... existing fields ...

    /// Maps class name → (HeapPtr to Class object, field names in order)
    pub class_registry: HashMap<String, (HeapPtr, Vec<String>)>,
}
```

**Implementation**:
- Populate `class_registry` during VM creation from bytecode
- `allocate_from_external` accesses via `vm.class_registry`

**Pros**:
- No signature changes to `allocate_from_external`
- VM becomes self-contained for allocation
- Simple lookup: `vm.class_registry.get(&class_name)`

**Cons**:
- Memory overhead: HashMap duplicated per VM instance
- Need to ensure registry stays in sync if classes can change
- Increases BexVm complexity

---

### Option 3: Make `allocate_from_external` an Instance Method

**Approach**: Change from static method to instance method on BexEngine.

```rust
impl BexEngine {
    fn allocate_from_external(
        &self,  // Now has access to self.snapshot
        vm: &mut BexVm,
        external: &BexExternalValue,
        guard: &EpochGuard<'_>,
    ) -> Value {
        match external {
            BexExternalValue::Instance { class_name, fields } => {
                // Can now access self.snapshot.classes
                let class_def = self.snapshot.classes.get(class_name)?;
                // ...
            }
            // ...
        }
    }
}
```

**Implementation**:
- Change `Self::allocate_from_external(...)` to `self.allocate_from_external(...)`
- Also need to store/access class HeapPtrs (see sub-option below)

**Sub-option**: Add a `class_ptrs: HashMap<String, HeapPtr>` field to BexEngine that's populated when classes are allocated to the heap.

**Pros**:
- Direct access to schema via `self.snapshot`
- Minimal new data structures (just need HeapPtr lookup)
- Follows existing patterns in BexEngine

**Cons**:
- Still need somewhere to store class name → HeapPtr mapping
- Recursive calls need `&self` available (should be fine since caller has it)
- Tighter coupling between allocation and engine

---

### Option 4: Pre-resolve Class Pointers in BexExternalValue

**Approach**: Include resolved pointers in the external value type.

```rust
pub enum BexExternalValue {
    Instance {
        class_name: String,
        class_ptr: Option<HeapPtr>,  // Pre-resolved by caller
        fields: IndexMap<String, BexExternalValue>,
    },
    // ...
}
```

**Implementation**:
- Callers (bridge FFI) resolve class pointers before passing to engine
- `allocate_from_external` uses pre-resolved pointer if available

**Pros**:
- Allocation becomes trivial
- No changes to allocate_from_external logic

**Cons**:
- Leaks VM internals (HeapPtr) into external interface
- Bridge FFI doesn't currently have access to HeapPtr
- Breaking change to BexExternalValue
- Not all code paths may be able to pre-resolve

---

### Option 5: Lazy Resolution via Global Index

**Approach**: Store a name → GlobalIndex mapping, look up class from globals.

```rust
pub struct BexVm {
    pub globals: GlobalPool,
    pub class_index: HashMap<String, GlobalIndex>,  // NEW
}
```

**Implementation**:
- When loading bytecode, record which global index each class is stored at
- `allocate_from_external` does: `name → index → globals[index] → Class object`

**Pros**:
- Leverages existing globals infrastructure
- Classes already live in globals
- Minimal new storage (just indices, not full objects)

**Cons**:
- Two-level indirection for lookup
- Need to ensure indices remain valid
- Still requires field order information from somewhere

---

## Comparison Matrix

| Criteria                  | Option 1 (Param) | Option 2 (VM table) | Option 3 (Instance method) | Option 4 (Pre-resolve) | Option 5 (Global index) |
|---------------------------|------------------|---------------------|---------------------------|------------------------|------------------------|
| Code changes              | Moderate         | Moderate            | Small                     | Large                  | Moderate               |
| Memory overhead           | Low (shared)     | Per-VM HashMap      | Low (shared)              | None                   | Per-VM HashMap         |
| API changes               | Internal only    | Internal only       | Internal only             | External type change   | Internal only          |
| Coupling                  | Low              | Low                 | Medium                    | High                   | Low                    |
| Recursive handling        | Pass-through     | Automatic           | Automatic                 | N/A                    | Automatic              |
| Field ordering source     | Param or schema  | VM table            | Schema                    | Caller provides        | Needs schema           |

---

## Recommendation

**Option 3 (Instance Method)** combined with a **class pointer cache** in BexEngine:

```rust
pub struct BexEngine {
    // ... existing fields ...

    /// Cache of class name → HeapPtr, populated when classes are allocated
    class_ptrs: HashMap<String, HeapPtr>,
}
```

**Rationale**:

1. **Minimal memory duplication**: The cache stores only HeapPtrs, not full class definitions
2. **Natural access pattern**: `self.snapshot.classes` has field order, `self.class_ptrs` has heap pointers
3. **No external API changes**: BexExternalValue stays the same
4. **Simple implementation**: Just add `&self` to the method and access existing data

**Implementation sketch**:

```rust
impl BexEngine {
    fn allocate_from_external(
        &self,
        vm: &mut BexVm,
        external: &BexExternalValue,
        guard: &EpochGuard<'_>,
    ) -> Result<Value, EngineError> {
        match external {
            BexExternalValue::Instance { class_name, fields } => {
                // 1. Get class pointer
                let class_ptr = self.class_ptrs.get(class_name)
                    .ok_or_else(|| EngineError::UnknownClass(class_name.clone()))?;

                // 2. Get field order from schema
                let class_def = self.snapshot.classes.get(class_name).unwrap();

                // 3. Allocate fields in correct order
                let mut field_values = Vec::with_capacity(class_def.fields.len());
                for field_def in &class_def.fields {
                    let field_value = fields.get(&field_def.name)
                        .map(|v| self.allocate_from_external(vm, v, guard))
                        .unwrap_or(Ok(Value::Null))?;
                    field_values.push(field_value);
                }

                // 4. Create instance
                Ok(vm.tlab.alloc(Object::Instance(Instance {
                    class: *class_ptr,
                    fields: field_values,
                })))
            }
            // ... other cases unchanged ...
        }
    }
}
```

---

## Open Questions

1. **When to populate `class_ptrs`?** During `BexEngine::new()` after classes are allocated to the heap.

2. **What about Variant (enum) allocation?** Same pattern - need enum name → pointer mapping.

3. **Error handling?** What if `class_name` doesn't exist in schema? Return `EngineError::UnknownClass`.

4. **Dynamic types?** If TypeBuilder adds classes at runtime, need to update `class_ptrs`.
