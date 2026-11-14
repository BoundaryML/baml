use std::collections::HashSet;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::{
    baml_value::{BamlNull, BamlValue},
    deserializer::{
        coercer::array_helper,
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, DeserializerMeta},
    },
    jsonish::CompletionState,
    sap_model::{NullTy, TyResolvedRef, TypeIdent, TypeRefDb, UnionTy},
};

type Parsed<'s, 'v, 't, N> = BamlValueWithFlags<'s, 'v, 't, N>;
type MemberResult<'s, 'v, 't, N> = Result<Option<Parsed<'s, 'v, 't, N>>, ParsingError>;

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for UnionTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError> {
        let all_variants = &target.variants;
        if all_variants.is_empty() {
            return Err(ctx.error_uninhabited());
        }

        // A union is partial when its matching member is; members without a partial parse
        // yield `None` below and drop out.
        let mut add_flags = Vec::new();
        if value.completion_state() == &CompletionState::Incomplete {
            add_flags.push(Flag::Incomplete);
        }

        let members: Vec<_> = all_variants
            .iter()
            .map(|v| ctx.db.resolve(v).ok())
            .collect();
        let order = member_order(ctx.union_variant_hint, 0..all_variants.len());
        let selected = select_member(ctx.db, &members, &order, |i| {
            let mut parsed = ctx
                .db
                .resolve(&all_variants[i])
                .map_err(|ident| ctx.error_type_resolution(ident))
                .and_then(|ty| TyResolvedRef::coerce(ctx, ty, value));
            if let Ok(Some(val)) = &mut parsed
                && val.score() == 0
            {
                // Add UnionMatch flag so subsequent array elements can use this hint
                val.add_flag(Flag::UnionMatch(i, vec![]));
            }
            parsed
        });

        match selected {
            Ok(val) => Ok(Some(val.with_flags(add_flags))),
            Err(tried) => {
                let variants = tried.into_iter().map(|(_, r)| r).collect();
                let best = array_helper::pick_best(ctx, TyResolvedRef::Union(target), variants);
                best.map(|v| v.map(|v| v.with_flags(add_flags)))
            }
        }
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<BamlValueWithFlags<'s, 'v, 't, N>> {
        let flags = match value.completion_state() {
            CompletionState::Incomplete => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            CompletionState::Complete => DeserializerConditions::new(),
        };

        if matches!(value, crate::jsonish::Value::Null) && target.is_optional(ctx.db) {
            return Some(BamlValueWithFlags::new(
                BamlValue::Null(BamlNull),
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyResolvedRef::Null(NullTy),
                },
            ));
        }

        let variants: Vec<TyResolvedRef<'t, N>> = target
            .variants
            .iter()
            .map(|v| ctx.db.resolve(v))
            .collect::<Result<_, _>>()
            .ok()?;

        // Indices stay in the original (unfiltered) index space so UnionMatch flags and hints
        // agree with coerce().
        let non_null =
            (0..variants.len()).filter(|&i| !matches!(variants[i], TyResolvedRef::Null(_)));
        let order = member_order(ctx.union_variant_hint, non_null);
        let members: Vec<_> = variants.iter().copied().map(Some).collect();
        let selected = select_member(ctx.db, &members, &order, |i| {
            let mut cast_result = TyResolvedRef::try_cast(ctx, variants[i], value);
            if let Some(val) = &mut cast_result {
                // Add the flag with the CORRECT original index before storing.
                // This prevents pick_best from adding a flag with wrong (filtered list) index.
                val.add_flag(Flag::UnionMatch(i, vec![]));
            }
            Ok(cast_result)
        });

        let mut filtered_options: Vec<_> = match selected {
            Ok(val) => return Some(val.with_flags(flags.flags)),
            Err(tried) => tried
                .into_iter()
                .filter_map(|(_, r)| r.ok().flatten())
                .collect(),
        };
        match filtered_options.len() {
            0 => None,
            1 => {
                let v = filtered_options.remove(0);
                // Flag already added above with correct index
                Some(v.with_flags(flags.flags))
            }
            // pick_best will see the existing UnionMatch flag and won't add a duplicate
            _ => array_helper::pick_best(
                ctx,
                TyResolvedRef::Union(target),
                filtered_options
                    .into_iter()
                    .map(|v| Ok(Some(v)))
                    .collect::<Vec<_>>(),
            )
            .ok()
            .flatten()
            .map(|v| v.with_flags(flags.flags)),
        }
    }
}

/// The order a union tries its members in: a hinted member (the one a previous list element
/// matched) first, then the rest in declaration order. Among perfect matches with no clear
/// winner, the first in this order wins.
fn member_order(hint: Option<usize>, members: impl Iterator<Item = usize>) -> Vec<usize> {
    let members: Vec<usize> = members.collect();
    let hint = hint.filter(|h| members.contains(h));
    hint.into_iter()
        .chain(members.into_iter().filter(|&i| Some(i) != hint))
        .collect()
}

/// Picks the union member a value parses as, trying members in `order` with `try_member`.
///
/// Returns the winning perfect (score-zero) match, or, when no member matched perfectly, every
/// member's result in index order for the caller's imperfect-match selection. Among perfect
/// matches:
///
/// 1. A tuple built from a real JSON array beats a list built from one (see
///    [`array_helper::defers_to_perfect_tuple`]), so `[1, 1]` becomes `(int, int)` rather than
///    `int[]`; when a perfect tuple exists, only perfect tuples remain in contention.
/// 2. A member strictly more specific than every other remaining one wins (see
///    [`is_strictly_more_specific`]): for `[1]`, `(1,)` beats `(int,)` in either order.
/// 3. Otherwise the first remaining one in `order` wins: `(int,) | (int?,)` gives `(int,)`.
///
/// The search stops at the first perfect match unless some other member could still beat it,
/// i.e. it is a list or another member is strictly more specific; only then are the remaining
/// members tried.
fn select_member<'s, 'v, 't, N: TypeIdent>(
    db: &'t TypeRefDb<'t, N>,
    members: &[Option<TyResolvedRef<'t, N>>],
    order: &[usize],
    mut try_member: impl FnMut(usize) -> MemberResult<'s, 'v, 't, N>,
) -> Result<Parsed<'s, 'v, 't, N>, Vec<(usize, MemberResult<'s, 'v, 't, N>)>> {
    fn perfect<'a, 's, 'v, 't, N: TypeIdent>(
        result: &'a MemberResult<'s, 'v, 't, N>,
    ) -> Option<&'a Parsed<'s, 'v, 't, N>> {
        match result {
            Ok(Some(val)) if val.score() == 0 => Some(val),
            _ => None,
        }
    }

    let mut results: Vec<Option<MemberResult<'s, 'v, 't, N>>> =
        members.iter().map(|_| None).collect();
    let mut first_perfect = None;
    for &i in order {
        let result = try_member(i);
        let is_perfect = perfect(&result).is_some();
        results[i] = Some(result);
        if is_perfect {
            first_perfect = Some(i);
            break;
        }
    }

    let Some(first) = first_perfect else {
        return Err(results
            .into_iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (i, r)))
            .collect());
    };

    let more_specific_than = |a: usize, b: usize| match (members[a], members[b]) {
        (Some(a), Some(b)) => is_strictly_more_specific(db, a, b),
        _ => false,
    };
    let first_val = results[first].as_ref().and_then(|r| perfect(r));
    let may_be_beaten = first_val.is_some_and(array_helper::defers_to_perfect_tuple)
        || order
            .iter()
            .any(|&j| j != first && more_specific_than(j, first));
    if may_be_beaten {
        for &i in order {
            if results[i].is_none() {
                results[i] = Some(try_member(i));
            }
        }
    }

    let mut contenders: Vec<(usize, Parsed<'s, 'v, 't, N>)> = order
        .iter()
        .filter_map(|&i| match results[i].take() {
            Some(Ok(Some(val))) if val.score() == 0 => Some((i, val)),
            _ => None,
        })
        .collect();
    if contenders
        .iter()
        .any(|(_, v)| array_helper::is_array_tuple(v))
    {
        contenders.retain(|(_, v)| array_helper::is_array_tuple(v));
    }
    let winner = (0..contenders.len())
        .find(|&c| {
            contenders
                .iter()
                .enumerate()
                .all(|(k, (j, _))| k == c || more_specific_than(contenders[c].0, *j))
        })
        .unwrap_or(0);
    Ok(contenders.swap_remove(winner).1)
}

/// Whether union member `a` is strictly more specific than member `b`: `a` is at least as
/// specific as `b` everywhere and `b` is not at least as specific as `a`.
///
/// This is SAP's own order for ranking perfect union matches, not subtyping: under invariance
/// `(1,)` is not a subtype of `(int,)`, nor `1[]` of `int[]`, yet SAP prefers the literal arm.
/// "`a` is at least as specific as `b`" holds when:
///
/// - `a` and `b` are the same type;
/// - `a` is a literal and `b` its base primitive (`1` and `int`, `"x"` and `string`, `true` and
///   `bool`, `1n` and `bigint`), or `a` is one variant of the enum `b`;
/// - recursively, slot by slot: tuples of equal arity compare slot by slot, lists by element,
///   maps by key and value, stream states by their value, and classes by field: two classes
///   compare when they have the same field names, field by field (a class is equal to itself);
/// - with unions on either side (a non-union type counts as a union of one member): every
///   member of `a` is at least as specific as some member of `b`, and every member of `b` has
///   some member of `a` at least as specific as it. So `1 | 2` is more specific than `int` and
///   `1?` than `int?`, but nullability is never ranked: `int` and `int?` are incomparable,
///   since `null` in `int?` has no counterpart in `int`.
///
/// Anything else is incomparable: different shapes (a tuple and a list, tuples of different
/// arity, classes with different fields), or slots that point in opposite directions, like
/// `(int, string?)` and `(int?, string)`. Incomparable members have no winner, so union order
/// decides. Recursive types compare coinductively: a pair already being compared is assumed
/// to hold.
pub(super) fn is_strictly_more_specific<'t, N: TypeIdent>(
    db: &'t TypeRefDb<'t, N>,
    a: TyResolvedRef<'t, N>,
    b: TyResolvedRef<'t, N>,
) -> bool {
    let mut cmp = SpecificityCmp {
        db,
        assumed: HashSet::new(),
    };
    cmp.at_least_as_specific(a, b) && !cmp.at_least_as_specific(b, a)
}

/// The "at least as specific" relation of [`is_strictly_more_specific`].
struct SpecificityCmp<'t, N: TypeIdent> {
    db: &'t TypeRefDb<'t, N>,
    /// Composite pairs being compared further up the stack, by address.
    assumed: HashSet<(usize, usize)>,
}

impl<'t, N: TypeIdent> SpecificityCmp<'t, N> {
    fn ty(
        &mut self,
        a: &'t crate::sap_model::Ty<'t, N>,
        b: &'t crate::sap_model::Ty<'t, N>,
    ) -> bool {
        match (self.db.resolve(a), self.db.resolve(b)) {
            (Ok(a), Ok(b)) => self.at_least_as_specific(a, b),
            _ => false,
        }
    }

    fn at_least_as_specific(&mut self, a: TyResolvedRef<'t, N>, b: TyResolvedRef<'t, N>) -> bool {
        if !matches!(a, TyResolvedRef::Union(_)) && !matches!(b, TyResolvedRef::Union(_)) {
            return self.member(a, b);
        }
        let (Some(a_members), Some(b_members)) = (self.flatten(a), self.flatten(b)) else {
            return false;
        };
        a_members
            .iter()
            .all(|&x| b_members.iter().any(|&y| self.member(x, y)))
            && b_members
                .iter()
                .all(|&y| a_members.iter().any(|&x| self.member(x, y)))
    }

    /// The non-union members of `ty`, through nested unions and aliases; `None` if a member
    /// does not resolve.
    fn flatten(&self, ty: TyResolvedRef<'t, N>) -> Option<Vec<TyResolvedRef<'t, N>>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![ty];
        while let Some(ty) = stack.pop() {
            match ty {
                TyResolvedRef::Union(u) => {
                    if seen.insert(std::ptr::from_ref(u).addr()) {
                        for v in u.variants.iter().rev() {
                            stack.push(self.db.resolve(v).ok()?);
                        }
                    }
                }
                other => out.push(other),
            }
        }
        Some(out)
    }

    /// Compares two non-union types.
    fn member(&mut self, a: TyResolvedRef<'t, N>, b: TyResolvedRef<'t, N>) -> bool {
        use TyResolvedRef as T;
        let key = match (address(a), address(b)) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        if let Some(key) = key {
            if self.assumed.contains(&key) {
                return true;
            }
            self.assumed.insert(key);
        }
        let result = match (a, b) {
            (T::Int(_), T::Int(_))
            | (T::Bigint(_), T::Bigint(_))
            | (T::Float(_), T::Float(_))
            | (T::String(_), T::String(_))
            | (T::Bool(_), T::Bool(_))
            | (T::Null(_), T::Null(_)) => true,
            (T::Media(x), T::Media(y)) => x == y,
            (T::LiteralInt(x), T::LiteralInt(y)) => x == y,
            (T::LiteralBigint(x), T::LiteralBigint(y)) => x == y,
            (T::LiteralString(x), T::LiteralString(y)) => x == y,
            (T::LiteralBool(x), T::LiteralBool(y)) => x == y,
            (T::LiteralInt(_), T::Int(_))
            | (T::LiteralBigint(_), T::Bigint(_))
            | (T::LiteralString(_), T::String(_))
            | (T::LiteralBool(_), T::Bool(_)) => true,
            (T::Enum(x), T::Enum(y)) => x == y,
            (T::EnumVariant(x), T::EnumVariant(y)) => x == y,
            (T::EnumVariant(x), T::Enum(y)) => x.name == y.name,
            (T::Array(x), T::Array(y)) => self.ty(&x.ty, &y.ty),
            (T::Tuple(x), T::Tuple(y)) => {
                x.items.len() == y.items.len()
                    && x.items.iter().zip(&y.items).all(|(x, y)| self.ty(x, y))
            }
            (T::Map(x), T::Map(y)) => self.ty(&x.key, &y.key) && self.ty(&x.value, &y.value),
            (T::StreamState(x), T::StreamState(y)) => self.ty(&x.value, &y.value),
            (T::Shaped(x), T::Shaped(y)) => x.name == y.name && self.ty(&x.shape, &y.shape),
            (T::Class(x), T::Class(y)) => {
                x == y
                    || (x.fields.len() == y.fields.len()
                        && x.fields.iter().all(|xf| {
                            y.fields
                                .iter()
                                .find(|yf| yf.name == xf.name)
                                .is_some_and(|yf| self.ty(&xf.ty, &yf.ty))
                        }))
            }
            (T::Union(_), _) | (_, T::Union(_)) => self.at_least_as_specific(a, b),
            _ => false,
        };
        if let Some(key) = key {
            self.assumed.remove(&key);
        }
        result
    }
}

/// The address of a composite type, which identifies it while comparing recursive types.
fn address<N: TypeIdent>(ty: TyResolvedRef<'_, N>) -> Option<usize> {
    use TyResolvedRef as T;
    Some(match ty {
        T::Array(x) => std::ptr::from_ref(x).addr(),
        T::Tuple(x) => std::ptr::from_ref(x).addr(),
        T::Map(x) => std::ptr::from_ref(x).addr(),
        T::Class(x) => std::ptr::from_ref(x).addr(),
        T::StreamState(x) => std::ptr::from_ref(x).addr(),
        T::Shaped(x) => std::ptr::from_ref(x).addr(),
        _ => return None,
    })
}
