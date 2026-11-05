//! How an instance of a class crosses the host boundary.

use baml_base::{LangItem, LangRoots, MediaKind, RustPayloadKind, lang_item};
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
    /// A host's own value (`baml.host.Value`), as the host's reference to
    /// it: the host that sent it resolves it back to its own object, and
    /// the program holds it but never looks into it.
    HostValue,
}

impl ClassCrossing {
    /// The crossing of the class `head`: a standard-library class with a
    /// rule of its own ([`BoundaryClass`]) crosses by that rule; any other
    /// class crosses by its fields. A `@@rust_type` class crosses by its kind
    /// instead ([`Self::of_kind`]).
    pub fn of_declaration(lang: LangRoots, head: &DeclName) -> Self {
        let is = |item: LangItem| DeclName::of_lang_item(lang, item).as_ref() == Some(head);
        BoundaryClass::ALL
            .into_iter()
            .find(|class| is(class.item()))
            .map_or(Self::Fields, BoundaryClass::crossing)
    }

    /// The crossing of a `@@rust_type` class of kind `kind`: a media value
    /// and a rendered prompt by their own value arms, a host's value as the
    /// host's reference, and any other state as a reference to the engine's
    /// instance.
    pub const fn of_kind(kind: RustPayloadKind) -> Self {
        match kind {
            RustPayloadKind::Image => Self::Media(MediaKind::Image),
            RustPayloadKind::Audio => Self::Media(MediaKind::Audio),
            RustPayloadKind::Pdf => Self::Media(MediaKind::Pdf),
            RustPayloadKind::Video => Self::Media(MediaKind::Video),
            RustPayloadKind::Prompt => Self::Prompt,
            RustPayloadKind::HostValue => Self::HostValue,
            RustPayloadKind::Foreign
            | RustPayloadKind::DefaultHasher
            | RustPayloadKind::Sha256
            | RustPayloadKind::Aes256GcmSiv
            | RustPayloadKind::Aes128GcmSiv
            | RustPayloadKind::ChaCha20Poly1305
            | RustPayloadKind::XChaCha20Poly1305
            | RustPayloadKind::Regex
            | RustPayloadKind::CsvRecord
            | RustPayloadKind::Limit
            | RustPayloadKind::CancelToken
            | RustPayloadKind::SpanId
            | RustPayloadKind::TraceOptions
            | RustPayloadKind::ReservedSpan
            | RustPayloadKind::File
            | RustPayloadKind::Glob
            | RustPayloadKind::TcpStream
            | RustPayloadKind::TcpListener
            | RustPayloadKind::UdpSocket
            | RustPayloadKind::Xoshiro256PlusPlus
            | RustPayloadKind::ChaCha20
            | RustPayloadKind::InterfaceWitness
            | RustPayloadKind::CsvReader
            | RustPayloadKind::CsvWriter
            | RustPayloadKind::ClassBuilder
            | RustPayloadKind::ChildProcess
            | RustPayloadKind::ReadPipe
            | RustPayloadKind::WritePipe
            | RustPayloadKind::WebSocket
            | RustPayloadKind::HttpBody
            | RustPayloadKind::SseStream
            | RustPayloadKind::HttpServer
            | RustPayloadKind::TlsKeys
            | RustPayloadKind::OutputFormat
            | RustPayloadKind::Lock => Self::Opaque,
        }
    }

    /// Whether the class's state is the runtime's own: a `@@rust_type` class
    /// (which crosses by reference or by a form of its own) or a capability
    /// such as `reflect.Package`. Such a class has no JSON form, hashes only
    /// through an explicit implementation, and no structural builder (a
    /// decoder, a parser, reflection) makes one. A stream and a function spec
    /// keep the structural reading their fields give them.
    #[must_use]
    pub const fn runtime_owned(self) -> bool {
        match self {
            Self::Opaque | Self::Media(_) | Self::Prompt | Self::HostValue => true,
            Self::Fields | Self::Stream | Self::FunctionSpec => false,
        }
    }

    /// Whether a host may build an instance from its fields.
    pub const fn host_builds(self) -> bool {
        match self {
            Self::Fields => true,
            Self::Opaque
            | Self::Stream
            | Self::FunctionSpec
            | Self::Media(_)
            | Self::Prompt
            | Self::HostValue => false,
        }
    }
}

/// The standard-library classes whose crossing is their own rather than
/// their fields', by identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoundaryClass {
    /// `ai.stream.Stream`.
    Stream,
    /// `ai.FunctionSpec`.
    FunctionSpec,
    /// `reflect.Package`.
    Package,
    /// `reflect.Session`.
    Session,
}

impl BoundaryClass {
    /// Every class with a rule of its own.
    pub const ALL: [Self; 4] = [
        Self::Stream,
        Self::FunctionSpec,
        Self::Package,
        Self::Session,
    ];

    /// The class, by identity.
    pub const fn item(self) -> LangItem {
        match self {
            Self::Stream => lang_item!(Ai: stream.Stream),
            Self::FunctionSpec => lang_item!(Ai: FunctionSpec),
            Self::Package => lang_item!(Reflect: Package),
            Self::Session => lang_item!(Reflect: Session),
        }
    }

    /// How an instance of the class crosses.
    pub const fn crossing(self) -> ClassCrossing {
        match self {
            Self::Stream => ClassCrossing::Stream,
            Self::FunctionSpec => ClassCrossing::FunctionSpec,
            Self::Package | Self::Session => ClassCrossing::Opaque,
        }
    }
}
