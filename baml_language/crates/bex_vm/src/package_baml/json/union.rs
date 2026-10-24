//! Which member of a union a JSON value decodes as: the best match.
//!
//! The candidates are the members a value of its JSON kind can decode as at
//! all: an object is never a `string`, a number never a class. Among them the
//! most specific is tried first, and the first that decodes wins:
//!
//! - an object prefers the member that uses the most of its keys. A class uses
//!   the keys that name its fields under the naming in effect, media its tag
//!   keys, and a map every key;
//! - a number prefers a literal, then `int`, then `bigint`, then `float`, so an
//!   integral number is an `int` when it fits;
//! - a string prefers a literal or a single enum variant, then an enum, then
//!   `string` (and a `bigint` read from a numeric string, which ties with it);
//! - `json`, `unknown`, and a map of either take any value, so they come last.
//!
//! Ties go to declaration order. A nested union or recursive alias ranks as
//! its best member, and resolves inside by the same rule when it is decoded.
//!
//! Only what decides the order is worked out up front: the JSON kind, and the
//! keys only when two or more candidates count them. A member that fails deep
//! inside falls through to the next.

use bex_vm_types::{HeapPtr, Object, RealizedTy};
use smallvec::SmallVec;

use super::plan::Plans;
use crate::BexVm;

/// What JSON value a union is decoding: what members are prefiltered by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

impl Kind {
    /// The kind of the JSON text whose first byte is `byte`.
    pub(super) fn of_text(byte: u8) -> Self {
        match byte {
            b'n' => Kind::Null,
            b't' | b'f' => Kind::Bool,
            b'"' => Kind::String,
            b'[' => Kind::Array,
            b'{' => Kind::Object,
            _ => Kind::Number,
        }
    }
}

/// The keys of the JSON object a union is decoding, to count against each
/// member that reads keys.
pub(super) enum Keys<'k> {
    /// A `json` object in memory: a string-keyed map.
    Map(HeapPtr),
    /// The object's keys as written, sorted and without repeats.
    Text(&'k [String]),
}

impl Keys<'_> {
    fn has(&self, vm: &BexVm, key: &str, hash: u64) -> bool {
        match self {
            Keys::Map(ptr) => super::object_get(vm, *ptr, hash, key).is_some(),
            Keys::Text(keys) => keys
                .binary_search_by(|written| written.as_str().cmp(key))
                .is_ok(),
        }
    }

    fn len(&self, vm: &BexVm) -> usize {
        match self {
            Keys::Map(ptr) => match vm.get_object(*ptr) {
                Object::Map(map) => map.len(),
                _ => 0,
            },
            Keys::Text(keys) => keys.len(),
        }
    }
}

/// How a member fits a value of one kind, before any decoding.
#[derive(Clone, Copy)]
enum Fit {
    /// Takes any value: `json`, `unknown`, or a map of either.
    Any,
    /// Fits values of this kind at a fixed rank: higher is more specific.
    Rank(u8),
    /// An object member, ranked by how many of the object's keys it uses.
    Keys,
}

/// The members of a union that can decode a value of one kind, each with how
/// it fits.
pub(super) struct Candidates(SmallVec<[(usize, Fit); 4]>);

impl Candidates {
    /// The members of `members` that can decode a value of kind `kind`.
    pub(super) fn of(vm: &BexVm, members: &[RealizedTy], kind: Kind) -> Self {
        Self(
            members
                .iter()
                .enumerate()
                .filter_map(|(index, member)| Some((index, fit(vm, member, kind)?)))
                .collect(),
        )
    }

    /// Whether the order depends on the object's keys: two or more
    /// candidates count them.
    pub(super) fn counts_keys(&self) -> bool {
        self.0
            .iter()
            .filter(|(_, fit)| matches!(fit, Fit::Keys))
            .nth(1)
            .is_some()
    }

    /// The candidates' indexes in `members`, in the order to try them, best
    /// first. `keys` are the object's keys when [`Self::counts_keys`].
    pub(super) fn order(
        self,
        vm: &BexVm,
        members: &[RealizedTy],
        plans: &mut Plans,
        keys: Option<&Keys<'_>>,
    ) -> SmallVec<[usize; 4]> {
        if self.0.len() <= 1 {
            return self.0.into_iter().map(|(index, _)| index).collect();
        }
        // `json` and the like below everything, then the kind's own ranks,
        // or the number of keys used.
        let mut ranked: SmallVec<[(usize, u64); 4]> = self
            .0
            .into_iter()
            .map(|(index, fit)| {
                let rank = match fit {
                    Fit::Any => 0,
                    Fit::Rank(rank) => 1 + u64::from(rank),
                    Fit::Keys => {
                        1 + keys
                            .map_or(0, |keys| used_keys(vm, &members[index], keys, plans) as u64)
                    }
                };
                (index, rank)
            })
            .collect();
        // Stable: ties keep declaration order.
        ranked.sort_by(|(_, a), (_, b)| b.cmp(a));
        ranked.into_iter().map(|(index, _)| index).collect()
    }
}

/// The member that is not `null`, when `members` is `T | null` in either
/// order: such a union needs no ranking, since `null` decodes as `null` and
/// anything else can only be a `T`.
pub(super) fn other_than_null(members: &[RealizedTy]) -> Option<&RealizedTy> {
    match members {
        [RealizedTy::Null, other] | [other, RealizedTy::Null] if !other.is_null() => Some(other),
        _ => None,
    }
}

/// How `member` fits a value of kind `kind`, or `None` when it cannot decode
/// one.
fn fit(vm: &BexVm, member: &RealizedTy, kind: Kind) -> Option<Fit> {
    use baml_type::Literal;
    if is_any(vm, member) {
        return Some(Fit::Any);
    }
    let rank = |fits: bool, rank: u8| fits.then_some(Fit::Rank(rank));
    match member {
        RealizedTy::Null => rank(kind == Kind::Null, 0),
        RealizedTy::Bool => rank(kind == Kind::Bool, 0),
        RealizedTy::Literal(Literal::Bool(_), _) => rank(kind == Kind::Bool, 1),
        RealizedTy::Float => rank(kind == Kind::Number, 0),
        RealizedTy::Bigint => match kind {
            Kind::Number => Some(Fit::Rank(1)),
            // A numeric string ties with `string`.
            Kind::String => Some(Fit::Rank(0)),
            _ => None,
        },
        RealizedTy::Int => rank(kind == Kind::Number, 2),
        RealizedTy::Literal(Literal::Int(_) | Literal::Float(_), _) => {
            rank(kind == Kind::Number, 3)
        }
        RealizedTy::Literal(Literal::Bigint(_), _) => {
            rank(matches!(kind, Kind::Number | Kind::String), 3)
        }
        RealizedTy::String => rank(kind == Kind::String, 0),
        RealizedTy::Enum(_) => rank(kind == Kind::String, 1),
        RealizedTy::Literal(Literal::String(_), _) | RealizedTy::EnumVariant(..) => {
            rank(kind == Kind::String, 2)
        }
        RealizedTy::List(_) => rank(kind == Kind::Array, 0),
        RealizedTy::Map { .. }
        | RealizedTy::Class(..)
        | RealizedTy::Interface(..)
        | RealizedTy::Media(_) => (kind == Kind::Object).then_some(Fit::Keys),
        RealizedTy::Union(_) | RealizedTy::TypeAlias(_) => {
            let mut inner = Vec::new();
            super::alias_members(vm, member, &mut Vec::new(), &mut inner);
            inner
                .iter()
                .filter_map(|inner| fit(vm, inner, kind))
                .reduce(|best, fit| match (best, fit) {
                    (Fit::Keys, _) | (_, Fit::Keys) => Fit::Keys,
                    (Fit::Any, other) | (other, Fit::Any) => other,
                    (Fit::Rank(a), Fit::Rank(b)) => Fit::Rank(a.max(b)),
                })
        }
        // No JSON form, or no values: these never decode.
        RealizedTy::Uint8Array
        | RealizedTy::Function { .. }
        | RealizedTy::Future(..)
        | RealizedTy::Resource
        | RealizedTy::PromptAst
        | RealizedTy::RustType
        | RealizedTy::Type
        | RealizedTy::Never => None,
        RealizedTy::Unknown => Some(Fit::Any),
    }
}

/// Whether `ty` takes any JSON value: `json`, `unknown`, or a map of either.
fn is_any(vm: &BexVm, ty: &RealizedTy) -> bool {
    let any = |ty: &RealizedTy| match ty {
        RealizedTy::Unknown => true,
        RealizedTy::TypeAlias(head) => vm.stdlib_heads().is_json_alias(*head),
        _ => false,
    };
    match ty {
        RealizedTy::Map { value, .. } => any(value),
        _ => any(ty),
    }
}

/// How many of the object's keys `member`, an object member, uses.
fn used_keys(vm: &BexVm, member: &RealizedTy, keys: &Keys<'_>, plans: &mut Plans) -> usize {
    match member {
        RealizedTy::Map { .. } => keys.len(vm),
        RealizedTy::Media(_) => media_keys(vm, keys),
        RealizedTy::Class(head, _) if vm.stdlib_heads().media_kind(*head).is_some() => {
            media_keys(vm, keys)
        }
        RealizedTy::Class(head, type_args) | RealizedTy::Interface(head, type_args, _) => {
            let Ok(plan) = plans.class(vm, *head, type_args) else {
                return 0;
            };
            let mut used = 0;
            for (index, field) in plan.fields.iter().enumerate() {
                // Two fields under one key use it once.
                let repeated = plan.shared_keys
                    && plan.fields[..index]
                        .iter()
                        .any(|other| other.shown && other.key == field.key);
                if field.shown && !repeated && keys.has(vm, &field.key, field.hash()) {
                    used += 1;
                }
            }
            used
        }
        RealizedTy::Union(_) | RealizedTy::TypeAlias(_) => {
            let mut inner = Vec::new();
            super::alias_members(vm, member, &mut Vec::new(), &mut inner);
            inner
                .iter()
                .filter(|inner| matches!(fit(vm, inner, Kind::Object), Some(Fit::Keys)))
                .map(|inner| used_keys(vm, inner, keys, plans))
                .max()
                .unwrap_or(0)
        }
        _ => 0,
    }
}

/// How many of a tagged media object's keys the object has.
fn media_keys(vm: &BexVm, keys: &Keys<'_>) -> usize {
    ["kind", "source", "value", "mime", "name"]
        .into_iter()
        .filter(|key| keys.has(vm, key, bex_vm_types::map_string_hash(key)))
        .count()
}
