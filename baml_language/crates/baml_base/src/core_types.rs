//! Core types used throughout the compiler.

use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use smol_str::SmolStr;
use text_size::{TextRange, TextSize};

/// Borsh adapters for `num_bigint::BigInt`, which has no native borsh impl.
/// Encoded as a length-prefixed little-endian two's-complement byte string —
/// `BigInt::to_signed_bytes_le` / `from_signed_bytes_le` are the canonical
/// binary form and round-trip without loss.
pub mod borsh_bigint {
    use borsh::{BorshDeserialize, BorshSerialize};
    use num_bigint::BigInt;

    pub fn serialize<W: std::io::Write>(value: &BigInt, writer: &mut W) -> std::io::Result<()> {
        let bytes = value.to_signed_bytes_le();
        BorshSerialize::serialize(&bytes, writer)
    }

    pub fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<BigInt> {
        let bytes = Vec::<u8>::deserialize_reader(reader)?;
        Ok(BigInt::from_signed_bytes_le(&bytes))
    }
}

/// Unique identifier for a source file.
///
/// ## Bit layout
///
/// ```text
///   3 3 2 2 2 2 2 2 2 2 2 2 1 1 1 1 1 1 1 1 1 1
///   1 0 9 8 7 6 5 4 3 2 1 0 9 8 7 6 5 4 3 2 1 0 9 8 7 6 5 4 3 2 1 0
///  ├─┬─┬─┬─┼───────────────────────────────────────────────────────────┤
///  │ tag   │                    index (28 bits)                        │
///  └───────┴──────────────────────────────────────────────────────────-┘
/// ```
///
/// - **tag `0x0`** — real file (assigned by the host database)
/// - **tag `0x1`** — synthetic stream expansion of the origin file at `index`
/// - **tag `0xF`** — sentinel / fake (used by `Span::fake()` and `Span::default()`)
///
/// ## Why not `enum FileId { Real(u32), Stream(u32), Sentinel }`?
///
/// `FileId` is stored inside every `Span` (`file_id` + `TextRange` = 12 bytes).
/// An enum would widen `FileId` from 4 to 8 bytes (discriminant + alignment),
/// inflating `Span` from 12 to 16 bytes — a 33% increase across millions of spans.
///
/// ## Prior art
///
/// - **Roslyn** (C#): synthetic `SyntaxTree`s constructed with a virtual file path.
/// - **Clang**: bit 31 of `SourceLocation` distinguishes file vs macro-expansion locs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, BorshSerialize, BorshDeserialize)]
pub struct FileId(u32);

impl FileId {
    /// Create a `FileId` for a real source file.
    ///
    /// # Panics
    /// Panics if `id` uses the top 4 bits (reserved for tags).
    pub fn new(id: u32) -> Self {
        assert!(
            id & 0xF000_0000 == 0,
            "FileId::new({id}) exceeds 28-bit limit — top 4 bits are reserved"
        );
        FileId(id)
    }

    /// Sentinel value for fake/default spans. Not a real file.
    ///
    /// Bypasses the `new()` assert — the sentinel uses tag `0xF`.
    pub fn sentinel() -> FileId {
        FileId(u32::MAX)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A span in source code, tracking both file and position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file_id: FileId,
    pub range: TextRange,
}

// `TextRange` (from the `text-size` crate) doesn't impl `BorshSerialize` /
// `BorshDeserialize`, so we write the impls by hand as `(start_u32, end_u32)`
// — the same shape `text-size`'s serde impl uses.
impl BorshSerialize for Span {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.file_id, writer)?;
        let start: u32 = self.range.start().into();
        let end: u32 = self.range.end().into();
        BorshSerialize::serialize(&start, writer)?;
        BorshSerialize::serialize(&end, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for Span {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let file_id = FileId::deserialize_reader(reader)?;
        let start = u32::deserialize_reader(reader)?;
        let end = u32::deserialize_reader(reader)?;
        // `TextRange::new` panics on `end < start`. A malformed envelope
        // should surface as a clean borsh error rather than a thread crash.
        if start > end {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid Span range: start ({start}) > end ({end})"),
            ));
        }
        Ok(Span {
            file_id,
            range: TextRange::new(TextSize::new(start), TextSize::new(end)),
        })
    }
}

impl Default for Span {
    /// Creates a sentinel span that doesn't refer to any real file.
    ///
    /// Uses `u32::MAX` as the file ID to avoid conflicts with real files.
    fn default() -> Self {
        Self::fake()
    }
}

impl Span {
    pub fn new(file_id: FileId, range: TextRange) -> Self {
        Span { file_id, range }
    }

    /// Create a fake span for testing or when no real span is available.
    ///
    /// Uses a sentinel `FileId` (`u32::MAX`) that's unlikely to conflict with real files.
    pub fn fake() -> Self {
        Span {
            file_id: FileId::sentinel(),
            range: TextRange::empty(TextSize::new(0)),
        }
    }
}

/// An interned string - used for identifiers, keywords, etc.
pub type Name = SmolStr;

/// A possibly-qualified type-path identifier as written in source
/// (e.g., `MyClass`, `baml.errors.Io`, `root.http.Response`).
///
/// Stored as a `Vec<Name>` so consumers (TIR resolution, MIR field-order
/// lookup) can read the segments directly, rather than re-splitting a dotted
/// `Name`. A bare name is `vec![n]` — `is_qualified` is the structural
/// `len() > 1` check, not a substring scan.
///
/// `Display` joins with `.` for diagnostics and for places that key off the
/// dotted form (the bytecode emitter's class registry, debug snapshots).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypePath(pub Vec<Name>);

impl TypePath {
    pub fn new(segments: Vec<Name>) -> Self {
        debug_assert!(
            !segments.is_empty(),
            "TypePath must have at least one segment"
        );
        Self(segments)
    }

    pub fn bare(name: Name) -> Self {
        Self(vec![name])
    }

    /// Build a `TypePath` from a compile-time dotted literal like `"ai.Prompt"`.
    /// Use only at synthetic construction sites; runtime input should come from
    /// already-segmented data (e.g., parser tokens).
    pub fn from_dotted(s: &str) -> Self {
        Self(s.split('.').map(Name::new).collect())
    }

    pub fn segments(&self) -> &[Name] {
        &self.0
    }

    pub fn is_qualified(&self) -> bool {
        self.0.len() > 1
    }

    /// The unqualified leaf (e.g., `Response` for `root.http.Response`).
    pub fn leaf(&self) -> &Name {
        self.0.last().expect("TypePath is non-empty")
    }
}

impl fmt::Display for TypePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for seg in &self.0 {
            if !first {
                f.write_str(".")?;
            }
            f.write_str(seg.as_str())?;
            first = false;
        }
        Ok(())
    }
}

/// The types of media we support
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Copy, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Pdf,
    Generic, // could be any of the media types
}

impl MediaKind {
    /// Tag value used in the BEP-038 `{ kind, source, value, mime }` JSON
    /// shape. `Generic` collapses to `"media"` (any media subtype).
    pub fn tag_str(self) -> &'static str {
        match self {
            MediaKind::Image => "image",
            MediaKind::Audio => "audio",
            MediaKind::Video => "video",
            MediaKind::Pdf => "pdf",
            MediaKind::Generic => "media",
        }
    }

    /// The stdlib wrapper class (`baml.media.*`) that carries this media kind
    /// as a nominal value; `None` for `Generic`, which has no wrapper class.
    ///
    /// Single source of truth for the kind ↔ wrapper-class mapping, together
    /// with [`MediaKind::from_wrapper_class_name`]. Consumers must resolve
    /// wrapper class names through these instead of local string matches.
    pub const fn wrapper_class_name(self) -> Option<&'static str> {
        match self {
            MediaKind::Image => Some("baml.media.Image"),
            MediaKind::Audio => Some("baml.media.Audio"),
            MediaKind::Video => Some("baml.media.Video"),
            MediaKind::Pdf => Some("baml.media.Pdf"),
            MediaKind::Generic => None,
        }
    }

    /// Inverse of [`MediaKind::wrapper_class_name`]: the media kind carried by
    /// a stdlib media wrapper class, or `None` for any other class name.
    pub fn from_wrapper_class_name(name: &str) -> Option<Self> {
        match name {
            "baml.media.Image" => Some(MediaKind::Image),
            "baml.media.Audio" => Some(MediaKind::Audio),
            "baml.media.Video" => Some(MediaKind::Video),
            "baml.media.Pdf" => Some(MediaKind::Pdf),
            _ => None,
        }
    }
}

#[cfg(test)]
mod media_kind_wrapper_tests {
    use super::MediaKind;

    #[test]
    fn wrapper_class_name_round_trips() {
        for kind in [
            MediaKind::Image,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::Pdf,
        ] {
            let name = kind
                .wrapper_class_name()
                .expect("concrete kind has a wrapper");
            assert_eq!(MediaKind::from_wrapper_class_name(name), Some(kind));
        }
        assert_eq!(MediaKind::Generic.wrapper_class_name(), None);
        assert_eq!(MediaKind::from_wrapper_class_name("baml.media.File"), None);
        assert_eq!(MediaKind::from_wrapper_class_name("user.Image"), None);
    }
}

impl fmt::Display for MediaKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MediaKind::Image | MediaKind::Audio | MediaKind::Video | MediaKind::Pdf => {
                write!(f, "{}", self.tag_str())
            }
            MediaKind::Generic => write!(f, "image | audio | video | pdf"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum Literal {
    Int(i64),
    Bigint(
        #[borsh(
            serialize_with = "borsh_bigint::serialize",
            deserialize_with = "borsh_bigint::deserialize"
        )]
        num_bigint::BigInt,
    ),
    Float(String),
    String(String),
    Bool(bool),
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(s) => write!(f, "{s:?}"),
            Literal::Int(i) => write!(f, "{i}"),
            Literal::Bigint(n) => write!(f, "{n}n"),
            Literal::Float(s) => write!(f, "{s}"),
            Literal::Bool(b) => write!(f, "{b}"),
        }
    }
}

/// Module identifier (for multi-file support)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleId(u32);

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}
