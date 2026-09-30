//! Stable tag assignments shared by CAS blob and snapshot hash formats 1 to 3.
//! Changing an assignment requires a format version change.

use baml_type::MediaKind;

use crate::{Description, Limit};

#[repr(u8)]
pub(super) enum RootTag {
    Value = 0,
    FunctionArgs = 1,
}

#[repr(u8)]
pub(super) enum ValueTag {
    Null = 0,
    OmittedArg = 1,
    Bool = 2,
    Int = 3,
    Float = 4,
    String = 5,
    Bigint = 6,
    Object = 7,
    Type = 8,
    Enum = 9,
    Truncated = 10,
    /// The root value of a child blob.
    External = 11,
    /// One object of a child blob that is not that blob's root.
    ExternalNode = 12,
}

#[repr(u8)]
pub(super) enum ObjectTag {
    Uint8Array = 0,
    List = 1,
    Map = 2,
    Instance = 3,
    Declaration = 4,
    Cell = 5,
    NonSnapshotableValue = 6,
    Descriptive = 7,
    Truncated = 8,
    Media = 9,
}

#[repr(u8)]
pub(super) enum MediaSourceTag {
    Url = 0,
    File = 1,
    Base64 = 2,
}

#[repr(u8)]
pub(super) enum TypeIdentityTag {
    Unresolved = 0,
    Resolved = 1,
}

/// What a hash is of. Strings need none: their digest is their content hash.
#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum HashDomain {
    Bigint = 2,
    Type = 3,
    Blob = 5,
    /// `uint8array` contents, hashed while they are copied.
    Uint8Array = 6,
    /// One member of a cycle with its references left out, to choose the
    /// cycle's root. Never stored.
    CycleRoot = 7,
}

pub(super) fn limit(value: Limit) -> u8 {
    match value {
        Limit::Values => 0,
        Limit::Objects => 1,
        Limit::Bytes => 2,
        Limit::Depth => 3,
    }
}

pub(super) fn description(value: Description) -> u8 {
    match value {
        Description::Function => 0,
        Description::Closure => 1,
        Description::BoundMethod => 2,
        Description::GenericFunction => 3,
        Description::HostFunction => 4,
        Description::Future => 5,
        Description::UnscheduledFuture => 6,
        Description::Package => 7,
        Description::Interface => 8,
        Description::Implementation => 9,
        Description::TypeAlias => 10,
        Description::Sentinel => 11,
    }
}

pub(super) fn media_kind(value: MediaKind) -> u8 {
    match value {
        MediaKind::Image => 0,
        MediaKind::Audio => 1,
        MediaKind::Video => 2,
        MediaKind::Pdf => 3,
        MediaKind::Generic => 4,
    }
}

/// The kind [`media_kind`] writes as `tag`.
pub(super) fn media_kind_of(tag: u8) -> Option<MediaKind> {
    Some(match tag {
        0 => MediaKind::Image,
        1 => MediaKind::Audio,
        2 => MediaKind::Video,
        3 => MediaKind::Pdf,
        4 => MediaKind::Generic,
        _ => return None,
    })
}

pub(super) fn bigint_sign(value: num_bigint::Sign) -> u8 {
    match value {
        num_bigint::Sign::Minus => 0,
        num_bigint::Sign::NoSign => 1,
        num_bigint::Sign::Plus => 2,
    }
}
