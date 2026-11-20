//! The tag of every kind of thing a blob holds is `btel-core`'s (`btel_core::cas::tags`), the
//! one assignment writers and readers share; changing one is a format version change there.
//! What is here maps the writer's own enums onto those tags.

use baml_type::ConcreteMediaKind;
pub(super) use btel_core::cas::tags::{
    BigintSignTag, DescriptionTag, HashDomain, LimitTag, MediaSourceTag, ObjectTag, RedactionTag,
    RootTag, ValueTag,
};

use crate::{Description, Limit};

pub(super) fn limit(value: Limit) -> u8 {
    (match value {
        Limit::Values => LimitTag::Values,
        Limit::Objects => LimitTag::Objects,
        Limit::Bytes => LimitTag::Bytes,
        Limit::Depth => LimitTag::Depth,
    }) as u8
}

pub(super) fn description(value: Description) -> u8 {
    (match value {
        Description::Function => DescriptionTag::Function,
        Description::Closure => DescriptionTag::Closure,
        Description::BoundMethod => DescriptionTag::BoundMethod,
        Description::GenericFunction => DescriptionTag::GenericFunction,
        Description::HostFunction => DescriptionTag::HostFunction,
        Description::Future => DescriptionTag::Future,
        Description::UnscheduledFuture => DescriptionTag::UnscheduledFuture,
        Description::Package => DescriptionTag::Package,
        Description::Interface => DescriptionTag::Interface,
        Description::Implementation => DescriptionTag::Implementation,
        Description::TypeAlias => DescriptionTag::TypeAlias,
        Description::Sentinel => DescriptionTag::Sentinel,
    }) as u8
}

/// A media value's kind. A value always has a concrete one; the type `media` (any of them) is
/// written only in a type's description.
pub(super) fn media_kind(value: ConcreteMediaKind) -> u8 {
    match value {
        ConcreteMediaKind::Image => btel_core::cas::MediaKind::Image,
        ConcreteMediaKind::Audio => btel_core::cas::MediaKind::Audio,
        ConcreteMediaKind::Video => btel_core::cas::MediaKind::Video,
        ConcreteMediaKind::Pdf => btel_core::cas::MediaKind::Pdf,
    }
    .tag()
}

pub(super) fn bigint_sign(value: num_bigint::Sign) -> u8 {
    (match value {
        num_bigint::Sign::Minus => BigintSignTag::Negative,
        num_bigint::Sign::NoSign => BigintSignTag::Zero,
        num_bigint::Sign::Plus => BigintSignTag::Positive,
    }) as u8
}
