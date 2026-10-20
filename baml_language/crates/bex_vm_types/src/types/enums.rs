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
    /// Where this enum is declared inside its package: its namespace path
    /// and item name (an enum minted through `reflect` has only the item
    /// name). All of its name it carries — see [`Self::name`]. Never an
    /// identity: that is `type_tag` and the declaration object itself.
    pub path: super::LocalName,

    /// This enum's head identity — assigned by the linker (`CLASS_BASE +` its
    /// object index in the image) or minted fresh by the grafter; in a unit,
    /// its own object operand — the identity a `TypeHead` referring to this
    /// enum compares by.
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

    /// What this declaration belongs to; see `Class::owner`.
    #[borsh(skip)]
    pub owner: super::Owner,

    /// The definition telemetry recorded for this enum; see
    /// `Class::telemetry_definition`.
    #[borsh(skip)]
    pub telemetry_definition: btel_types::DefinitionCell,
}

impl Enum {
    /// How this enum is named: its path under its package's display name, or
    /// the bare path for one no package owns. Display and boundary spelling
    /// only — see [`declaration_name`](super::declaration_name).
    #[must_use]
    pub fn name(&self) -> crate::DeclarationName {
        self.owner.name_of(&self.path)
    }
}

impl std::fmt::Display for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name())
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
