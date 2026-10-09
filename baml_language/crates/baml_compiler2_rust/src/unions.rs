//! Type tests on union values.
//!
//! A union value is a generated enum ([`NativeTy::Union`]), so a type test
//! on one (`is_type`, `narrow_bind`, a `switch` on its `type_tag`) decides
//! which variants the value may hold. The analyzer resolves every test to a
//! [`MemberTest`] once ([`crate::function::Candidate::member_tests`]) and
//! the printer emits a `matches!` over the variants.

use baml_compiler2_mir::BlockId;
use baml_type::{Literal, typetag};

use crate::types::NativeTy;

/// What a type test decides on a union or nullable operand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemberTest {
    /// The value holds one of these variants (indices into the union's
    /// members); empty when no member can match, so the test is `false`.
    /// On a nullable operand without a union (`T | null`), the one index 0
    /// means the value is not `null`.
    Variants(Vec<usize>),
    /// The value holds variant `variant` and equals `literal`.
    Literal { variant: usize, literal: Literal },
    /// The value holds the enum member `variant` with the variant `index`.
    EnumVariant { variant: usize, index: usize },
    /// The value is `null` (the operand is nullable).
    Null,
}

/// Where a type test sits in the MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TestSite {
    /// The `is_type` / `is_type_tag` rvalue of statement `1` of block `0`.
    Statement(BlockId, usize),
    /// The `narrow_bind` ending the block.
    Terminator(BlockId),
    /// Arm `1` of the `switch` ending block `0`, on a union's type tag.
    SwitchArm(BlockId, usize),
}

/// The VM's type tag of values of this member type (`baml_type::typetag`),
/// for a `switch` on a union's `type_tag`; `None` for a class, whose tag is
/// the declaration's and which a switch names by [`SwitchKey::Class`].
///
/// [`SwitchKey::Class`]: baml_compiler2_mir::SwitchKey::Class
pub(crate) fn member_tag(member: &NativeTy<'_>) -> Option<i64> {
    Some(match member {
        NativeTy::Int => typetag::INT,
        NativeTy::Str => typetag::STRING,
        NativeTy::Bool => typetag::BOOL,
        NativeTy::Null => typetag::NULL,
        NativeTy::Float => typetag::FLOAT,
        NativeTy::Bigint => typetag::BIGINT,
        // Every enum shares one tag.
        NativeTy::Enum(_) => typetag::ENUM,
        NativeTy::Array(_) => typetag::LIST,
        NativeTy::Map(..) => typetag::MAP,
        NativeTy::Class(_)
        | NativeTy::Option(_)
        | NativeTy::Union(_)
        | NativeTy::Fn(..)
        | NativeTy::ArrayIter(_)
        | NativeTy::Thrown => return None,
    })
}

/// The variants of `members` whose values carry `tag`: a coarse test, as
/// the VM's is (`int[]` and `string[]` both carry the list tag).
pub(crate) fn tag_variants(members: &[NativeTy<'_>], tag: i64) -> Vec<usize> {
    members
        .iter()
        .enumerate()
        .filter(|(_, member)| member_tag(member) == Some(tag))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_select_members_by_kind() {
        let members = [
            NativeTy::Int,
            NativeTy::Array(Box::new(NativeTy::Int)),
            NativeTy::Array(Box::new(NativeTy::Str)),
            NativeTy::Float,
        ];
        assert_eq!(tag_variants(&members, typetag::INT), vec![0]);
        assert_eq!(tag_variants(&members, typetag::LIST), vec![1, 2]);
        assert_eq!(tag_variants(&members, typetag::STRING), Vec::<usize>::new());
        assert_eq!(member_tag(&NativeTy::Option(Box::new(NativeTy::Int))), None);
    }
}
