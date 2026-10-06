//! Recorded class and enum definitions, independent of any query engine.
//!
//! A recording publishes a definition for each class or enum its captured
//! values name, keyed by the engine-scoped type tag the values carry
//! (`Definitions.types`, format minor 10). What a reader needs from that:
//!
//! - [`DefinitionRow::from_wire`]: what to store for one published entry, and
//!   [`DefinitionRow::supersedes`]: the merge rule (a declaration replaces an
//!   unavailable observation; the first declaration is kept), so entries from
//!   files indexed in any order converge.
//! - [`definition_id`]: the id a rendered reference carries, `<recording
//!   id>:<tag>`. Every reference to a class or enum has one, recorded or not.
//! - [`TypeDefinitions`]: what rendering asks a store for, lazily, one tag at
//!   a time. A query engine keeps the rows wherever it likes and implements
//!   this over them.

use std::sync::Arc;

use baml_type::typetag::TypeTag;
pub use btel_recorder::decode_declaration;
use btel_recorder::proto;
pub use btel_types::TypeDeclaration;

/// What a reader stores per (recording, tag).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionRow {
    pub state: DefinitionState,
    /// The encoded `proto::TypeDeclaration`, when declared.
    pub declaration: Option<Vec<u8>>,
}

/// Ordered: a later state supersedes an earlier one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefinitionState {
    /// The runtime could not copy the definition.
    Unavailable = 1,
    /// The definition is recorded.
    Declared = 2,
}

impl DefinitionState {
    /// The integer a store may keep.
    #[must_use]
    pub fn code(self) -> i64 {
        self as i64
    }
}

impl DefinitionRow {
    /// The row one published entry stores; `None` for an empty entry.
    #[must_use]
    pub fn from_wire(definition: &proto::TypeDefinition) -> Option<Self> {
        use proto::type_definition::Resolution;
        match definition.resolution.as_ref()? {
            Resolution::Declaration(declaration) => Some(Self {
                state: DefinitionState::Declared,
                declaration: Some(prost::Message::encode_to_vec(declaration)),
            }),
            Resolution::Unavailable(_) => Some(Self {
                state: DefinitionState::Unavailable,
                declaration: None,
            }),
        }
    }

    /// Whether this row replaces a stored row in `existing` state.
    #[must_use]
    pub fn supersedes(&self, existing: Option<DefinitionState>) -> bool {
        existing.is_none_or(|existing| existing < self.state)
    }

    /// The declaration a stored row holds.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<TypeDeclaration> {
        let declaration = <proto::TypeDeclaration as prost::Message>::decode(bytes).ok()?;
        decode_declaration(&declaration)
    }
}

/// The id of `tag`'s definition in the recording `recording_id` (its 16
/// bytes): unique across recordings, the same for every reference in one.
#[must_use]
pub fn definition_id(recording_id: &[u8], tag: TypeTag) -> Arc<str> {
    format!("{}:{}", crate::discovery::hex(recording_id), tag.as_i64()).into()
}

/// The definitions of one recording, as rendering asks for them.
pub trait TypeDefinitions {
    /// The id every reference to `tag` carries.
    fn id(&self, tag: TypeTag) -> Arc<str>;
    /// The recorded definition, if any. Called at most once per tag per
    /// rendered cell; implementations may load lazily and cache.
    fn declaration(&self, tag: TypeTag) -> Option<Arc<TypeDeclaration>>;
}
