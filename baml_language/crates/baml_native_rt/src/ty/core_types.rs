// Copied from crates/baml_base/src/core_types.rs: `MediaKind` and `Literal`, with
// layouts the compiler can write. A change to either file must be made to both.

//! Core types used throughout the compiler.

use std::cmp::Ordering;

use super::{CowSlice, Text};

/// The types of media we support
#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, PartialOrd, Ord)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Pdf,
    Generic, // could be any of the media types
}

/// `Bigint` is a count of limbs and the limbs, two's complement, least
/// significant first, as a `bigint` object holds them. `Float` is the
/// literal's text.
///
/// Equal, ordered and hashed by value, as `num_bigint::BigInt` is: a
/// `bigint` compares by the number it is, whatever its count of limbs.
#[repr(C, u8)]
#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Bigint(CowSlice<u64>),
    Float(Text),
    String(Text),
    Bool(bool),
}

impl Literal {
    /// The position of the variant in the declaration: what a derived `Ord`
    /// compares first.
    fn rank(&self) -> u8 {
        match self {
            Literal::Int(_) => 0,
            Literal::Bigint(_) => 1,
            Literal::Float(_) => 2,
            Literal::String(_) => 3,
            Literal::Bool(_) => 4,
        }
    }
}

/// The limbs of a `bigint` without the top limbs that only extend the sign:
/// one form for each number.
fn bigint_trimmed(limbs: &[u64]) -> &[u64] {
    let mut len = limbs.len();
    while len > 1 {
        let top = limbs[len - 1];
        let below_negative = limbs[len - 2] >> 63 == 1;
        if (top == 0 && !below_negative) || (top == u64::MAX && below_negative) {
            len -= 1;
        } else {
            break;
        }
    }
    &limbs[..len]
}

/// The numeric order of two `bigint`s given by their limbs.
fn bigint_cmp(left: &[u64], right: &[u64]) -> Ordering {
    let (left, right) = (bigint_trimmed(left), bigint_trimmed(right));
    let negative = |limbs: &[u64]| limbs.last().is_some_and(|top| top >> 63 == 1);
    match (negative(left), negative(right)) {
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }
    // Of the same sign, trimmed: more limbs is further from zero.
    let further = if negative(left) {
        Ordering::Less
    } else {
        Ordering::Greater
    };
    match left.len().cmp(&right.len()) {
        Ordering::Greater => further,
        Ordering::Less => further.reverse(),
        // Of the same sign and length, the limbs compare as unsigned words
        // from the most significant.
        Ordering::Equal => left.iter().rev().cmp(right.iter().rev()),
    }
}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Literal {}

impl PartialOrd for Literal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Literal {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Literal::Int(a), Literal::Int(b)) => a.cmp(b),
            (Literal::Bigint(a), Literal::Bigint(b)) => bigint_cmp(a, b),
            (Literal::Float(a), Literal::Float(b)) | (Literal::String(a), Literal::String(b)) => {
                a.cmp(b)
            }
            (Literal::Bool(a), Literal::Bool(b)) => a.cmp(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }
}

impl std::hash::Hash for Literal {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.rank().hash(state);
        match self {
            Literal::Int(n) => n.hash(state),
            Literal::Bigint(limbs) => bigint_trimmed(limbs).hash(state),
            Literal::Float(text) | Literal::String(text) => text.hash(state),
            Literal::Bool(b) => b.hash(state),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::bigint_cmp;

    #[test]
    fn bigints_compare_by_value_whatever_their_limbs() {
        let minus_one = [u64::MAX];
        let zero = [0];
        let two_to_the_64 = [0, 1];
        let minus_two_to_the_64 = [0, u64::MAX];
        assert_eq!(bigint_cmp(&minus_one, &zero), Ordering::Less);
        assert_eq!(bigint_cmp(&zero, &two_to_the_64), Ordering::Less);
        assert_eq!(bigint_cmp(&minus_two_to_the_64, &minus_one), Ordering::Less);
        assert_eq!(bigint_cmp(&[5, 0], &[5]), Ordering::Equal);
        assert_eq!(
            bigint_cmp(&[u64::MAX, u64::MAX], &minus_one),
            Ordering::Equal
        );
        assert_eq!(bigint_cmp(&[1 << 63, 0], &[u64::MAX]), Ordering::Greater);
    }
}
