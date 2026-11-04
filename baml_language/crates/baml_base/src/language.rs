//! Compiler-owned registry for built-in language constructs.
//!
//! This module contains semantic metadata shared by parsing, validation, and
//! editor tooling. Human-readable documentation lives in `baml_builtins2` and
//! is keyed by the stable names exposed here.

use borsh::{BorshDeserialize, BorshSerialize};

/// The argument shape accepted by a built-in schema attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaAttributeArguments {
    None,
    String {
        placeholder: &'static str,
    },
    /// No argument, or one string from a closed list. The lowering checks
    /// membership, so a carrier only ever stores a listed choice.
    OptionalChoice {
        choices: &'static [&'static str],
        placeholder: &'static str,
    },
}

/// The kind of Rust-owned state a `@@rust_type` class keeps, one per runtime
/// payload variant. Bare `@@rust_type` is [`Self::Foreign`]; every other kind
/// is named by its spelling: `@@rust_type("OrderedSet")`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub enum RustPayloadKind {
    /// A value the VM cannot look inside, reached only through its `Arc`.
    /// Transitional: the kind of every class that has no kind of its own yet.
    Foreign,
    DefaultHasher,
    Sha256,
    Aes256GcmSiv,
    Aes128GcmSiv,
    ChaCha20Poly1305,
    XChaCha20Poly1305,
    Regex,
    CsvRecord,
    Limit,
    CancelToken,
    SpanId,
    TraceOptions,
    ReservedSpan,
    /// A `baml.fs.File`: a platform's open file, reached through the
    /// operations its resource trait declares.
    File,
    /// A `baml.glob.Glob`: a platform's compiled pattern.
    Glob,
    /// A `baml.net.TcpStream`: a platform's open connection.
    TcpStream,
    /// A `baml.net.TcpListener`: a platform's bound listener.
    TcpListener,
    /// A `baml.net.UdpSocket`: a platform's bound socket.
    UdpSocket,
    /// A `baml.random.Xoshiro256PlusPlus`'s generator state.
    Xoshiro256PlusPlus,
    /// A `baml.random.ChaCha20`'s generator state.
    ChaCha20,
    /// A `reflect.interface.Implementation`'s witness, being built.
    InterfaceWitness,
    /// A `baml.csv.Reader`'s state: its options, input and position.
    CsvReader,
    /// A `baml.csv.Writer`'s state: its options, counts and buffer.
    CsvWriter,
    /// A `reflect.class.Builder`'s state: the class it is defining.
    ClassBuilder,
    /// A `baml.sys.Process`'s child: a platform's running process.
    ChildProcess,
    /// A `baml.sys.ReadPipe`: a platform's pipe to read from.
    ReadPipe,
    /// A `baml.sys.WritePipe`: a platform's pipe to write to.
    WritePipe,
    /// A `baml.ws.WebSocket`: a platform's connected socket.
    WebSocket,
    /// A `baml.http.Response`'s body: a platform's response body.
    HttpBody,
    /// A `baml.http.SseStream`'s state: a platform's event stream.
    SseStream,
    /// A `baml.http.Server`'s state: its bound listener.
    HttpServer,
    /// A `baml.http.TlsConfig`'s certificate chain and private key.
    TlsKeys,
}

impl RustPayloadKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 33] = [
        Self::Foreign,
        Self::DefaultHasher,
        Self::Sha256,
        Self::Aes256GcmSiv,
        Self::Aes128GcmSiv,
        Self::ChaCha20Poly1305,
        Self::XChaCha20Poly1305,
        Self::Regex,
        Self::CsvRecord,
        Self::Limit,
        Self::CancelToken,
        Self::SpanId,
        Self::TraceOptions,
        Self::ReservedSpan,
        Self::File,
        Self::Glob,
        Self::TcpStream,
        Self::TcpListener,
        Self::UdpSocket,
        Self::Xoshiro256PlusPlus,
        Self::ChaCha20,
        Self::InterfaceWitness,
        Self::CsvReader,
        Self::CsvWriter,
        Self::ClassBuilder,
        Self::ChildProcess,
        Self::ReadPipe,
        Self::WritePipe,
        Self::WebSocket,
        Self::HttpBody,
        Self::SseStream,
        Self::HttpServer,
        Self::TlsKeys,
    ];

    /// The spellings `@@rust_type("…")` accepts, in [`Self::ALL`] order.
    pub const SPELLINGS: &'static [&'static str] = &[
        "DefaultHasher",
        "Sha256",
        "Aes256GcmSiv",
        "Aes128GcmSiv",
        "ChaCha20Poly1305",
        "XChaCha20Poly1305",
        "Regex",
        "CsvRecord",
        "Limit",
        "CancelToken",
        "SpanId",
        "TraceOptions",
        "ReservedSpan",
        "File",
        "Glob",
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "Xoshiro256PlusPlus",
        "ChaCha20",
        "InterfaceWitness",
        "CsvReader",
        "CsvWriter",
        "ClassBuilder",
        "ChildProcess",
        "ReadPipe",
        "WritePipe",
        "WebSocket",
        "HttpBody",
        "SseStream",
        "HttpServer",
        "TlsKeys",
    ];

    /// How this kind is named in `@@rust_type("…")`, or `None` for the kind
    /// bare `@@rust_type` means. The spelling is also the name of the kind's
    /// payload type and runtime variant.
    pub fn spelling(self) -> Option<&'static str> {
        match self {
            Self::Foreign => None,
            Self::DefaultHasher => Some("DefaultHasher"),
            Self::Sha256 => Some("Sha256"),
            Self::Aes256GcmSiv => Some("Aes256GcmSiv"),
            Self::Aes128GcmSiv => Some("Aes128GcmSiv"),
            Self::ChaCha20Poly1305 => Some("ChaCha20Poly1305"),
            Self::XChaCha20Poly1305 => Some("XChaCha20Poly1305"),
            Self::Regex => Some("Regex"),
            Self::CsvRecord => Some("CsvRecord"),
            Self::Limit => Some("Limit"),
            Self::CancelToken => Some("CancelToken"),
            Self::SpanId => Some("SpanId"),
            Self::TraceOptions => Some("TraceOptions"),
            Self::ReservedSpan => Some("ReservedSpan"),
            Self::File => Some("File"),
            Self::Glob => Some("Glob"),
            Self::TcpStream => Some("TcpStream"),
            Self::TcpListener => Some("TcpListener"),
            Self::UdpSocket => Some("UdpSocket"),
            Self::Xoshiro256PlusPlus => Some("Xoshiro256PlusPlus"),
            Self::ChaCha20 => Some("ChaCha20"),
            Self::InterfaceWitness => Some("InterfaceWitness"),
            Self::CsvReader => Some("CsvReader"),
            Self::CsvWriter => Some("CsvWriter"),
            Self::ClassBuilder => Some("ClassBuilder"),
            Self::ChildProcess => Some("ChildProcess"),
            Self::ReadPipe => Some("ReadPipe"),
            Self::WritePipe => Some("WritePipe"),
            Self::WebSocket => Some("WebSocket"),
            Self::HttpBody => Some("HttpBody"),
            Self::SseStream => Some("SseStream"),
            Self::HttpServer => Some("HttpServer"),
            Self::TlsKeys => Some("TlsKeys"),
        }
    }

    /// The kind `@@rust_type("spelling")` names.
    pub fn from_spelling(spelling: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.spelling() == Some(spelling))
    }
}

/// Where a schema attribute is written. Attributes belong to declarations: a
/// member position takes `@name` trailing the member, a block position takes
/// `@@name` inside the declaration's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributePosition {
    /// `@name` trailing a class field.
    ClassField,
    /// `@name` trailing an interface field.
    InterfaceField,
    /// `@name` trailing an enum variant.
    EnumVariant,
    /// `@@name` inside a class body.
    Class,
    /// `@@name` inside an enum body.
    Enum,
    /// `@@name` inside an interface body.
    Interface,
    /// `@@name` on a function, method, or interface method signature.
    Function,
}

impl AttributePosition {
    /// The prefix an attribute is written with at this position.
    pub const fn sigil(self) -> &'static str {
        match self {
            Self::ClassField | Self::InterfaceField | Self::EnumVariant => "@",
            Self::Class | Self::Enum | Self::Interface | Self::Function => "@@",
        }
    }

    /// The position as a diagnostic reads it, article included.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::ClassField => "a class field",
            Self::InterfaceField => "an interface field",
            Self::EnumVariant => "an enum variant",
            Self::Class => "a class",
            Self::Enum => "an enum",
            Self::Interface => "an interface",
            Self::Function => "a function",
        }
    }
}

/// Semantic metadata for a built-in schema attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaAttributeSpec {
    pub name: &'static str,
    pub arguments: SchemaAttributeArguments,
    pub repeatable: bool,
    /// Written only in the standard library: a user file is refused, and the
    /// menus and diagnostics shown to a user never list it.
    pub builtin_only: bool,
    /// The positions the attribute has meaning at. Anywhere else it is an
    /// error: an attribute the compiler knows but does not read at a position
    /// would otherwise be accepted and silently ignored.
    pub positions: &'static [AttributePosition],
}

impl SchemaAttributeSpec {
    /// Render the attribute without its contextual `@`/`@@` prefix.
    pub fn signature(self) -> String {
        match self.arguments {
            SchemaAttributeArguments::None => self.name.to_string(),
            SchemaAttributeArguments::String { placeholder }
            | SchemaAttributeArguments::OptionalChoice { placeholder, .. } => {
                format!(r#"{}("{placeholder}")"#, self.name)
            }
        }
    }

    pub fn allows(self, position: AttributePosition) -> bool {
        self.positions.contains(&position)
    }
}

// No schema attribute has meaning on an interface or its fields (a contract,
// not a concrete data type: nothing is ever parsed or rendered *as* one) or on
// a function. Those positions exist so that writing one there is an error
// rather than an accepted no-op.
const DATA_MEMBERS_AND_BLOCKS: &[AttributePosition] = &[
    AttributePosition::ClassField,
    AttributePosition::EnumVariant,
    AttributePosition::Class,
    AttributePosition::Enum,
];

const DATA_MEMBERS: &[AttributePosition] = &[
    AttributePosition::ClassField,
    AttributePosition::EnumVariant,
];

pub const SCHEMA_ATTRIBUTE_SPECS: &[SchemaAttributeSpec] = &[
    SchemaAttributeSpec {
        name: "description",
        arguments: SchemaAttributeArguments::String {
            placeholder: "text",
        },
        repeatable: false,
        builtin_only: false,
        positions: DATA_MEMBERS_AND_BLOCKS,
    },
    SchemaAttributeSpec {
        name: "alias",
        arguments: SchemaAttributeArguments::String {
            placeholder: "name",
        },
        repeatable: false,
        builtin_only: false,
        positions: DATA_MEMBERS_AND_BLOCKS,
    },
    SchemaAttributeSpec {
        name: "skip",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        builtin_only: false,
        positions: DATA_MEMBERS,
    },
    // BEP-075: the streaming attributes are declaration attributes. A field's
    // `@stream.done` holds the field at its default until its value is
    // complete; a class's `@@stream.done` holds the whole object.
    SchemaAttributeSpec {
        name: "stream.done",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        builtin_only: false,
        positions: &[AttributePosition::ClassField, AttributePosition::Class],
    },
    // The field has no default, so its class has no partial parse until the
    // field is present.
    SchemaAttributeSpec {
        name: "stream.must_exist",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
        builtin_only: false,
        positions: &[AttributePosition::ClassField],
    },
    // The one attribute here that is about the language rather than the
    // schema: the class keeps its state in Rust, of the kind named.
    SchemaAttributeSpec {
        name: "rust_type",
        arguments: SchemaAttributeArguments::OptionalChoice {
            choices: RustPayloadKind::SPELLINGS,
            placeholder: "Kind",
        },
        repeatable: false,
        builtin_only: true,
        positions: &[AttributePosition::Class],
    },
];

pub fn schema_attribute_spec(name: &str) -> Option<&'static SchemaAttributeSpec> {
    SCHEMA_ATTRIBUTE_SPECS.iter().find(|spec| spec.name == name)
}

/// The schema attributes that have meaning at `position`, in table order.
pub fn schema_attribute_specs_at(
    position: AttributePosition,
) -> impl Iterator<Item = &'static SchemaAttributeSpec> {
    SCHEMA_ATTRIBUTE_SPECS
        .iter()
        .filter(move |spec| spec.allows(position))
}

/// The attributes a user may write at `position`: those with meaning there
/// that are not builtin-only. What a completion menu offers and what an
/// unknown-attribute diagnostic suggests.
pub fn user_attribute_specs_at(
    position: AttributePosition,
) -> impl Iterator<Item = &'static SchemaAttributeSpec> {
    schema_attribute_specs_at(position).filter(|spec| !spec.builtin_only)
}

/// Presentation-neutral identity and signature for a well-known client
/// configuration key. Provider-specific validation remains owned by the
/// client-options/type schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientConfigKeySpec {
    pub name: &'static str,
    pub signature: &'static str,
}

pub const CLIENT_CONFIG_KEY_SPECS: &[ClientConfigKeySpec] = &[
    ClientConfigKeySpec {
        name: "provider",
        signature: "provider <name>",
    },
    ClientConfigKeySpec {
        name: "options",
        signature: "options { ... }",
    },
    ClientConfigKeySpec {
        name: "model",
        signature: "model <name>",
    },
    ClientConfigKeySpec {
        name: "http",
        signature: "http { ... }",
    },
    ClientConfigKeySpec {
        name: "request_timeout_ms",
        signature: "request_timeout_ms <milliseconds>",
    },
    ClientConfigKeySpec {
        name: "retry_policy",
        signature: "retry_policy <name>",
    },
];

pub fn client_config_key_spec(name: &str) -> Option<&'static ClientConfigKeySpec> {
    CLIENT_CONFIG_KEY_SPECS
        .iter()
        .find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_payload_spellings_are_the_kinds_spellings_in_order() {
        let spelled: Vec<&str> = RustPayloadKind::ALL
            .into_iter()
            .filter_map(RustPayloadKind::spelling)
            .collect();
        assert_eq!(spelled, RustPayloadKind::SPELLINGS);
        for kind in RustPayloadKind::ALL {
            if let Some(spelling) = kind.spelling() {
                assert_eq!(RustPayloadKind::from_spelling(spelling), Some(kind));
            }
        }
    }

    #[test]
    fn schema_attribute_lookup_and_signatures() {
        assert_eq!(
            schema_attribute_spec("alias").map(|spec| spec.signature()),
            Some(r#"alias("name")"#.to_string())
        );
        assert_eq!(
            schema_attribute_spec("skip").map(|spec| spec.signature()),
            Some("skip".to_string())
        );
        assert_eq!(
            schema_attribute_spec("stream.done").map(|spec| spec.signature()),
            Some("stream.done".to_string())
        );
        assert!(schema_attribute_spec("stream.with_state").is_none());
    }

    #[test]
    fn schema_attribute_positions() {
        let names_at = |position| {
            schema_attribute_specs_at(position)
                .map(|spec| spec.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names_at(AttributePosition::ClassField),
            [
                "description",
                "alias",
                "skip",
                "stream.done",
                "stream.must_exist"
            ]
        );
        assert_eq!(
            names_at(AttributePosition::EnumVariant),
            ["description", "alias", "skip"]
        );
        assert_eq!(
            names_at(AttributePosition::Class),
            ["description", "alias", "stream.done", "rust_type"]
        );
        // A user is never offered what a user cannot write.
        assert_eq!(
            user_attribute_specs_at(AttributePosition::Class)
                .map(|spec| spec.name)
                .collect::<Vec<_>>(),
            ["description", "alias", "stream.done"]
        );
        assert_eq!(names_at(AttributePosition::Enum), ["description", "alias"]);
        assert!(names_at(AttributePosition::Interface).is_empty());
        assert!(names_at(AttributePosition::Function).is_empty());
        assert!(names_at(AttributePosition::InterfaceField).is_empty());
    }

    #[test]
    fn client_config_key_lookup() {
        assert_eq!(
            client_config_key_spec("request_timeout_ms").map(|spec| spec.signature),
            Some("request_timeout_ms <milliseconds>")
        );
        assert!(client_config_key_spec("temperature").is_none());
    }
}
