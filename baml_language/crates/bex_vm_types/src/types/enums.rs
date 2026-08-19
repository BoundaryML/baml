use borsh::{BorshDeserialize, BorshSerialize};
use indexmap::IndexMap;

use crate::HeapPtr;

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

    /// This enum's head identity, content-addressed from its fully-qualified
    /// name at emit time — the identity a `TypeHead` referring to this enum
    /// compares by.
    ///
    /// Distinct from the `TypeTag` instruction's dispatch value: every enum
    /// *value* reports the shared `type_tags::ENUM`, since dispatch does not
    /// currently discriminate between enums. Per-enum dispatch could use this,
    /// but that is an emitter change and not what this field is for.
    pub type_tag: baml_type::typetag::TypeTag,

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
    /// The runtime package that owns this declaration, or null for a
    /// compile-time one. A GC edge; see `Class::owner`.
    #[borsh(skip)]
    pub owner: HeapPtr,
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
