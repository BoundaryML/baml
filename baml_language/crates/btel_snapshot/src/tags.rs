//! Stable tag assignments shared by CAS blob and snapshot hash formats 1 to 3.
//! Changing an assignment requires a format version change.

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
}

#[repr(u8)]
pub(super) enum TypeIdentityTag {
    Unresolved = 0,
    Resolved = 1,
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum HashDomain {
    String = 1,
    Bigint = 2,
    Type = 3,
    Object = 4,
    Blob = 5,
    /// `uint8array` contents, hashed while they are copied.
    Uint8Array = 6,
    /// One member of a cycle with its references left out, to choose the
    /// cycle's root. Never stored.
    CycleRoot = 7,
    Range = 32,
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

pub(super) fn bigint_sign(value: num_bigint::Sign) -> u8 {
    match value {
        num_bigint::Sign::Minus => 0,
        num_bigint::Sign::NoSign => 1,
        num_bigint::Sign::Plus => 2,
    }
}
