//! Stable tag assignments for CAS blob format 1 and snapshot hash format 1.
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
}

#[repr(u8)]
pub(super) enum ObjectTag {
    Bytes = 0,
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

#[repr(u8)]
pub(super) enum HashDomain {
    String = 1,
    Bigint = 2,
    Type = 3,
    Object = 4,
    Snapshot = 5,
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
