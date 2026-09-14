use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{AtomicValueSlot, CleanupLatch, HeapPtr, RuntimeTy, Value, types::TypeValue};

/// A field within a runtime class, carrying type and schema metadata.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ClassField {
    pub name: String,
    /// Used by paths that don't care about parametric class type-args
    /// (codegen, `sys_ops` walking, output-format rendering).  For typed
    /// runtime walking against an `Instance::class_type_args` binding, use
    /// `field_template` and call `substitute` on it instead.
    pub field_type: RuntimeTy,
    /// Field-type template with `TypeArgRef(N)` leaves for class-level
    /// generic params (`N` indexes into `Instance::class_type_args`).
    ///
    /// Populated by emit using the enclosing class's `generic_params`.  For
    /// non-generic classes this is a fully-realized template (no `TypeArgRef`).
    pub field_template: crate::TyTemplate,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,
    pub skip: bool,

    /// The exact `type` operand a runtime-constructed field was built from, so
    /// reflection reads back the definitions it carried, not just its shape.
    #[borsh(skip)]
    pub runtime_type: Option<TypeValue>,
}

/// Runtime class representation.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Class {
    /// How this declaration is named: package-qualified for a compiled class,
    /// a bare item name for a runtime-created one. Display and boundary
    /// spelling only — identity is `type_tag` and the declaration object
    /// itself. Use `name.display_name()` for the display string (e.g.
    /// "ai.PromptMessage" or "Person").
    pub name: crate::DeclarationName,

    /// Class fields with type and schema metadata.
    pub fields: Vec<ClassField>,

    /// Class-level description for LLM prompt schema rendering.
    pub description: Option<String>,

    /// Class-level serialization alias.
    pub alias: Option<String>,

    /// Class-level source documentation and custom annotations.
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,

    /// This class's head identity, content-addressed from its fully-qualified
    /// name at emit time. Both the `TypeTag` instruction's jump-table dispatch
    /// value and the identity a `TypeHead` referring to this class compares by.
    pub type_tag: baml_type::typetag::TypeTag,

    /// Class-level type attribute (e.g., from @@stream.done).
    pub ty_attr: baml_type::TyAttr,

    /// BEP-042: `true` if this class defines a magic `cleanup(self) -> void`
    /// finalizer. Set at emit time. The GC checks this bit to decide whether an
    /// instance is finalizable — so the common case (no `cleanup`) is a single
    /// flag read and never enters finalization.
    pub has_cleanup: bool,

    /// Number of generic params the class itself declares (`GenericBox<T>` ⇒ 1,
    /// non-generic ⇒ 0). A method's `display_type_params` are De Bruijn-ordered
    /// as *class params first, then the method's own*, so this count is the
    /// length of that class prefix — it lets the engine split a method's own
    /// generic params (which Gate A must demand) from the inherited class params
    /// (bound by the receiver, never by name). Set at emit time.
    pub generic_param_count: usize,

    /// The runtime package that owns this declaration, or null for a
    /// compile-time one. A GC edge: reaching the class keeps its package — and
    /// so its globals and dependencies — alive. Mirrors `InterfaceDef::owner`
    /// and `TypeAliasDef::owner`.
    #[borsh(skip)]
    pub owner: HeapPtr,
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Runtime instance representation.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Instance {
    /// Pointer to the class object in the heap.
    pub class: HeapPtr,

    /// Resolved class-level type args at construction time.  Empty when the
    /// class is non-generic.  De Bruijn-ordered to match
    /// `enclosing_generic_params()`: index 0 = first class param, etc.
    ///
    /// Boxed (immutable after construction) rather than a `Vec` so `Instance`
    /// stays within `Object`'s 64-byte budget once the `cleaned` latch is added
    /// — matching the existing `Box<[RuntimeTy]>` convention for type-arg lists.
    pub class_type_args: Box<[crate::RealizedTy]>,

    /// Fields are accessed by index. No string lookups. Each slot is atomic so
    /// racing field reads/writes across `spawn` fibers cannot become a Rust
    /// data race.
    pub fields: Vec<AtomicValueSlot>,

    /// BEP-042 `cleanup` run-once latch. `false` for a fresh instance; flipped
    /// `true` by the first `cleanup` invocation (explicit, `defer`, or the GC
    /// finalizer). Only classes with a `cleanup` method ever read or write it;
    /// for every other instance it is an inert `false`.
    pub cleaned: CleanupLatch,
}

impl Instance {
    pub fn new(
        class: HeapPtr,
        class_type_args: Box<[crate::RealizedTy]>,
        fields: Vec<Value>,
    ) -> Self {
        Self {
            class,
            class_type_args,
            fields: fields.into_iter().map(AtomicValueSlot::new).collect(),
            cleaned: CleanupLatch::new(false),
        }
    }

    #[inline]
    pub fn field_len(&self) -> usize {
        self.fields.len()
    }

    #[inline]
    pub fn try_load_field(&self, idx: usize) -> Option<Value> {
        self.fields.get(idx).map(AtomicValueSlot::load)
    }

    #[inline]
    pub fn load_field(&self, idx: usize) -> Value {
        self.fields[idx].load()
    }

    #[inline]
    pub fn try_store_field(&self, idx: usize, value: Value) -> Result<(), usize> {
        let Some(field) = self.fields.get(idx) else {
            return Err(self.fields.len());
        };
        field.store(value);
        Ok(())
    }

    #[inline]
    pub fn store_field(&self, idx: usize, value: Value) {
        self.fields[idx].store(value);
    }

    pub fn field_values(&self) -> impl Iterator<Item = Value> + '_ {
        self.fields.iter().map(AtomicValueSlot::load)
    }
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<instance of {:p}>", self.class.as_ptr())
    }
}
