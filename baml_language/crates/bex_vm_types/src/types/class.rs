use baml_type::RuntimeTy;
use borsh::{BorshDeserialize, BorshSerialize};

use crate::{AtomicValueSlot, HeapPtr, Value};

/// A field within a runtime class, carrying type and schema metadata.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ClassField {
    pub name: String,
    /// Resolved field type with `TypeVar`s erased to `RuntimeTy::BuiltinUnknown`.
    ///
    /// Used by paths that don't care about parametric class type-args
    /// (codegen, `sys_ops` walking, output-format rendering).  For typed
    /// runtime walking against an `Instance::class_type_args` binding, use
    /// `field_template` and call `substitute` on it instead.
    pub field_type: RuntimeTy,
    /// Field-type template with `TypeArgRef(N)` leaves for class-level
    /// generic params (`N` indexes into `Instance::class_type_args`).
    ///
    /// Populated by emit using the enclosing class's `generic_params`.  For
    /// non-generic classes this is `TyTemplate::Concrete(field_type.clone())`.
    pub field_template: baml_type::TyTemplate,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// Runtime class representation.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Class {
    /// Type identity: carries short name, module path, and display name.
    /// Use `name.display_name` for the display string (e.g. "baml.llm.OrchestrationStep" or "Person").
    pub name: baml_type::TypeName,

    /// Class fields with type and schema metadata.
    pub fields: Vec<ClassField>,

    /// Class-level description for LLM prompt schema rendering.
    pub description: Option<String>,

    /// Class-level serialization alias.
    pub alias: Option<String>,

    /// Type tag for this class, used by `TypeTag` instruction for jump table dispatch.
    /// Assigned during codegen as `CLASS_BASE + class_index`.
    pub type_tag: i64,

    /// Class-level type attribute (e.g., from @@stream.done).
    pub ty_attr: baml_type::TyAttr,
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
    pub class_type_args: Vec<baml_type::RuntimeTy>,

    /// Fields are accessed by index. No string lookups. Each slot is atomic so
    /// racing field reads/writes across `spawn` fibers cannot become a Rust
    /// data race.
    pub fields: Vec<AtomicValueSlot>,
}

impl Instance {
    pub fn new(
        class: HeapPtr,
        class_type_args: Vec<baml_type::RuntimeTy>,
        fields: Vec<Value>,
    ) -> Self {
        Self {
            class,
            class_type_args,
            fields: fields.into_iter().map(AtomicValueSlot::new).collect(),
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
