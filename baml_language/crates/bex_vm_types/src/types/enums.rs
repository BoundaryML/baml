use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::{HeapPtr, types::RuntimeTypeProvenance};

/// A variant within a runtime enum, carrying schema metadata.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct EnumVariant {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,
    pub skip: bool,
}

/// Runtime enum representation.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Enum {
    /// Type identity: carries short name, module path, and display name.
    /// Use `name.display_name` for the display string.
    pub name: baml_type::TypeName,

    /// Enum variants with schema metadata.
    pub variants: Vec<EnumVariant>,

    /// Enum-level description.
    pub description: Option<String>,

    /// Enum-level serialization alias.
    pub alias: Option<String>,

    /// Enum-level source documentation and custom annotations.
    pub docstring: Option<String>,
    pub other: IndexMap<String, String>,

    /// Enum-level type attribute.
    pub ty_attr: baml_type::TyAttr,

    /// Present only on runtime-constructed enums; omitted from program wire
    /// data for the same reason as `Class::runtime_type`.
    #[borsh(skip)]
    pub runtime_type: Option<RuntimeTypeProvenance>,
}

impl std::fmt::Display for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name)
    }
}

/// Same as [`crate::Instance`] but for enums.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Variant {
    /// Pointer to the enum object in the heap.
    pub enm: HeapPtr,

    /// Index of the variant in the ordered list of variants.
    pub index: usize,
}

impl std::fmt::Display for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<variant of {:p}>", self.enm.as_ptr())
    }
}
