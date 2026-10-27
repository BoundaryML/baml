//! How an instance of a class crosses the host boundary.

use baml_base::{LangItem, LangRoots, MediaKind, lang_item};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::DeclName;

/// How an instance of a class crosses the host boundary, decided once per
/// declaration by the compiler ([`Self::of_declaration`]) and read by the
/// engine, the code generators and the host-side walker alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum ClassCrossing {
    /// By its fields: a host reads an instance as plain data and may build
    /// one.
    Fields,
    /// As a reference to the engine's instance alone: a standard-library
    /// capability whose fields are the runtime's own (a compiled package, a
    /// session), which a host holds and passes back but never reads into.
    Opaque,
    /// The live `ai.stream.Stream` capability, as a reference.
    Stream,
    /// The live `ai.FunctionSpec` capability, as a reference.
    FunctionSpec,
    /// A media value, carried by its own wire arm.
    Media(MediaKind),
    /// A rendered `ai.Prompt`, carried by its own wire arm.
    Prompt,
}

impl ClassCrossing {
    /// The crossing of the class `head`: a standard-library class with a
    /// rule of its own ([`BoundaryClass`], a media wrapper) crosses by that
    /// rule; any other class crosses by its fields.
    pub fn of_declaration(lang: LangRoots, head: &DeclName) -> Self {
        let is = |item: LangItem| DeclName::of_lang_item(lang, item).as_ref() == Some(head);
        if let Some(kind) = MediaKind::ALL
            .into_iter()
            .find(|kind| kind.wrapper_class().is_some_and(is))
        {
            return Self::Media(kind);
        }
        BoundaryClass::ALL
            .into_iter()
            .find(|class| is(class.item()))
            .map_or(Self::Fields, BoundaryClass::crossing)
    }

    /// Whether a host may build an instance from its fields.
    pub const fn host_builds(self) -> bool {
        match self {
            Self::Fields => true,
            Self::Opaque | Self::Stream | Self::FunctionSpec | Self::Media(_) | Self::Prompt => {
                false
            }
        }
    }
}

/// The standard-library classes whose crossing is their own rather than
/// their fields', by identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoundaryClass {
    /// `ai.Prompt`.
    Prompt,
    /// `ai.stream.Stream`.
    Stream,
    /// `ai.FunctionSpec`.
    FunctionSpec,
    /// `reflect.Package`.
    Package,
    /// `reflect.Session`.
    Session,
    /// `reflect.CompileArtifact`.
    CompileArtifact,
}

impl BoundaryClass {
    /// Every class with a rule of its own.
    pub const ALL: [Self; 6] = [
        Self::Prompt,
        Self::Stream,
        Self::FunctionSpec,
        Self::Package,
        Self::Session,
        Self::CompileArtifact,
    ];

    /// The class, by identity.
    pub const fn item(self) -> LangItem {
        match self {
            Self::Prompt => lang_item!(Ai: Prompt),
            Self::Stream => lang_item!(Ai: stream.Stream),
            Self::FunctionSpec => lang_item!(Ai: FunctionSpec),
            Self::Package => lang_item!(Reflect: Package),
            Self::Session => lang_item!(Reflect: Session),
            Self::CompileArtifact => lang_item!(Reflect: CompileArtifact),
        }
    }

    /// How an instance of the class crosses.
    pub const fn crossing(self) -> ClassCrossing {
        match self {
            Self::Prompt => ClassCrossing::Prompt,
            Self::Stream => ClassCrossing::Stream,
            Self::FunctionSpec => ClassCrossing::FunctionSpec,
            Self::Package | Self::Session | Self::CompileArtifact => ClassCrossing::Opaque,
        }
    }
}
