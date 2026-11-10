//! One body per builtin, written once over the slot representation `E`.
//!
//! Generated adapters (`adapters_generated.rs`) monomorphize these at
//! [`RawWord`](bex_vm_types::layout::RawWord), [`ObjPtr`] and [`Tagged`].
//! A body's name is its declaration's `fn_name`; its parameters follow the
//! declaration in ABI types. Semantics match the `Value` bodies in
//! `package_baml` (`array.rs`, `map.rs`, `string.rs`).

#![allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_ref_mut,
    reason = "one calling convention for every body: generated adapters call each \
              `fn(&mut R, ...) -> Result<_, Fault>` alike"
)]

use bex_vm_types::{
    format_float,
    layout::{
        Elem, Fault, ListOf, Lists, Maps, NeedsVm, ObjPtr, ObjectModel, Shape, StrKey, Strings,
        Tagged,
    },
    map_string_hash,
};

use super::support::{alloc_tagged_list, elems_as, list_elem_slot};
use crate::array_index::{resolve_index, resolve_insert_index, resolve_slice_bound};

/// A length as a BAML `int`.
fn int_len(len: usize) -> i64 {
    i64::try_from(len).unwrap_or(i64::MAX)
}

/// The elements `[start, end)` of `items` after `slice`'s bound rules.
fn slice_bounds(start: i64, end: i64, len: usize) -> std::ops::Range<usize> {
    let start = resolve_slice_bound(start, len);
    // An `end` resolving before `start` yields an empty slice.
    let end = resolve_slice_bound(end, len).max(start);
    start..end
}

// ── Array ────────────────────────────────────────────────────────────────

pub(crate) fn baml_array_length<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
) -> Result<i64, Fault> {
    Ok(int_len(rt.list_len(array)))
}

pub(crate) fn baml_array_at<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    index: i64,
) -> Result<Option<E>, Fault> {
    let len = rt.list_len(array);
    Ok(resolve_index(index, len).and_then(|index| rt.list_get(array, index)))
}

pub(crate) fn baml_array_push<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    item: E,
) -> Result<i64, Fault> {
    let len = rt.list_len(array);
    rt.list_splice(array, len..len, &[item]);
    Ok(int_len(rt.list_len(array)))
}

pub(crate) fn baml_array_pop<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
) -> Result<Option<E>, Fault> {
    let len = rt.list_len(array);
    let Some(last) = len.checked_sub(1) else {
        return Ok(None);
    };
    Ok(rt.list_splice(array, last..len, &[]).pop())
}

pub(crate) fn baml_array_remove_at<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    index: i64,
) -> Result<Option<E>, Fault> {
    let Some(index) = resolve_index(index, rt.list_len(array)) else {
        return Ok(None);
    };
    Ok(rt.list_splice(array, index..index + 1, &[]).pop())
}

pub(crate) fn baml_array_concat<R: ObjectModel + Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    other: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let slot = list_elem_slot(rt, array)?;
    let mut elems = Lists::<E>::list_to_vec(rt, array);
    elems.extend(elems_as::<R, E>(rt, other, slot)?);
    Ok(Lists::<E>::list_alloc(rt, ListOf::SameAs(array), &elems))
}

pub(crate) fn baml_array_reverse<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let mut elems = rt.list_to_vec(array);
    elems.reverse();
    Ok(rt.list_alloc(ListOf::SameAs(array), &elems))
}

pub(crate) fn baml_array_slice<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    start: i64,
    end: i64,
) -> Result<ObjPtr, Fault> {
    let elems = rt.list_to_vec(array);
    let range = slice_bounds(start, end, elems.len());
    Ok(rt.list_alloc(ListOf::SameAs(array), &elems[range]))
}

pub(crate) fn baml_array_clear<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
) -> Result<(), Fault> {
    rt.list_splice(array, 0..usize::MAX, &[]);
    Ok(())
}

pub(crate) fn baml_array_shift<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
) -> Result<Option<E>, Fault> {
    if rt.list_len(array) == 0 {
        return Ok(None);
    }
    Ok(rt.list_splice(array, 0..1, &[]).pop())
}

pub(crate) fn baml_array_unshift<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    item: E,
) -> Result<i64, Fault> {
    rt.list_splice(array, 0..0, &[item]);
    Ok(int_len(rt.list_len(array)))
}

pub(crate) fn baml_array_insert<R: Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    item: E,
    idx: i64,
) -> Result<i64, Fault> {
    let len = rt.list_len(array);
    // A negative `idx` counts from the end; `[0, len]` is the valid range.
    let Some(at) = resolve_insert_index(idx, len) else {
        return Err(Fault::InvalidArgument {
            message: format!("array.insert: idx ({idx}) is out of range for length {len}"),
        });
    };
    rt.list_splice(array, at..at, &[item]);
    Ok(int_len(rt.list_len(array)))
}

/// A scalar or string as `join` renders it; anything else may have a
/// `to_string` override.
fn render_plain<R: ObjectModel>(rt: &R, value: Tagged) -> Result<String, Fault> {
    match value {
        Tagged::Null => Ok("null".to_string()),
        Tagged::Bool(boolean) => Ok(boolean.to_string()),
        Tagged::Int(int) => Ok(int.to_string()),
        Tagged::Float(float) => Ok(format_float(float)),
        Tagged::Ptr(ptr) if rt.shape(ptr) == Shape::Str => Ok(rt.str_text(ptr).to_string()),
        Tagged::Ptr(_) => Err(Fault::NeedsVm(NeedsVm::Render)),
    }
}

pub(crate) fn baml_array_join<R: ObjectModel + Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    separator: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let slot = list_elem_slot(rt, array)?;
    let parts = Lists::<E>::list_to_vec(rt, array)
        .into_iter()
        .map(|elem| render_plain(rt, elem.to_tagged(slot)))
        .collect::<Result<Vec<String>, Fault>>()?;
    let joined = parts.join(rt.str_text(separator));
    Ok(rt.str_alloc(&joined))
}

pub(crate) fn baml_array_map<R: ObjectModel + Lists<E>, E: Elem>(
    rt: &mut R,
    array: ObjPtr,
    f: &mut dyn FnMut(&mut R, E) -> Result<Tagged, Fault>,
    out: ListOf,
) -> Result<ObjPtr, Fault> {
    let results = Lists::<E>::list_to_vec(rt, array)
        .into_iter()
        .map(|elem| f(rt, elem))
        .collect::<Result<Vec<Tagged>, Fault>>()?;
    alloc_tagged_list(rt, out, results)
}

// ── Map ──────────────────────────────────────────────────────────────────

fn str_key<R: Strings + ?Sized>(rt: &R, key: ObjPtr) -> StrKey {
    StrKey {
        ptr: key,
        hash: map_string_hash(rt.str_text(key)),
    }
}

pub(crate) fn baml_map_length<R: Maps<E>, E: Elem>(rt: &mut R, map: ObjPtr) -> Result<i64, Fault> {
    Ok(int_len(rt.map_len(map)))
}

pub(crate) fn baml_map_has<R: Maps<E> + Strings, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
    key: ObjPtr,
) -> Result<bool, Fault> {
    Ok(Maps::<E>::map_get(rt, map, str_key(rt, key)).is_some())
}

pub(crate) fn baml_map_get<R: Maps<E> + Strings, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
    key: ObjPtr,
) -> Result<Option<E>, Fault> {
    Ok(rt.map_get(map, str_key(rt, key)))
}

pub(crate) fn baml_map_index<R: Maps<E> + Strings, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
    key: ObjPtr,
) -> Result<E, Fault> {
    rt.map_get(map, str_key(rt, key))
        .ok_or(Fault::MapKeyNotFound)
}

pub(crate) fn baml_map_set<R: Maps<E> + Strings, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
    key: ObjPtr,
    value: E,
) -> Result<Option<E>, Fault> {
    let key = str_key(rt, key);
    Ok(rt.map_set(map, key, value))
}

pub(crate) fn baml_map_delete<R: Maps<E> + Strings, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
    key: ObjPtr,
) -> Result<Option<E>, Fault> {
    let key = str_key(rt, key);
    Ok(rt.map_remove(map, key))
}

pub(crate) fn baml_map_get_or_insert<R: Maps<E> + Strings, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
    key: ObjPtr,
    default: E,
) -> Result<E, Fault> {
    let key = str_key(rt, key);
    if let Some(existing) = rt.map_get(map, key) {
        return Ok(existing);
    }
    rt.map_set(map, key, default);
    Ok(default)
}

pub(crate) fn baml_map_clear<R: Maps<E>, E: Elem>(rt: &mut R, map: ObjPtr) -> Result<(), Fault> {
    rt.map_clear(map);
    Ok(())
}

pub(crate) fn baml_map_keys<R: ObjectModel + Maps<E>, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let keys = Maps::<E>::map_keys(rt, map);
    alloc_tagged_list(rt, ListOf::KeysOf(map), keys)
}

pub(crate) fn baml_map_values<R: Maps<E> + Lists<E>, E: Elem>(
    rt: &mut R,
    map: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let values = rt.map_values(map);
    Ok(rt.list_alloc(ListOf::ValuesOf(map), &values))
}

// ── String ───────────────────────────────────────────────────────────────

/// Allocate each part and a `string[]` holding them.
fn alloc_strings<R: Strings + Lists<ObjPtr>>(rt: &mut R, parts: &[String]) -> ObjPtr {
    let ptrs: Vec<ObjPtr> = parts.iter().map(|part| rt.str_alloc(part)).collect();
    rt.list_alloc(ListOf::Strings, &ptrs)
}

/// Each character as its own string.
fn char_strings(text: &str) -> Vec<String> {
    text.chars().map(String::from).collect()
}

pub(crate) fn baml_string_char_count<R: Strings>(rt: &mut R, string: ObjPtr) -> Result<i64, Fault> {
    Ok(int_len(rt.str_char_len(string)))
}

pub(crate) fn baml_string_split<R: Strings + Lists<ObjPtr>>(
    rt: &mut R,
    string: ObjPtr,
    delimiter: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let text = rt.str_text(string);
    let delimiter = rt.str_text(delimiter);
    let parts = if delimiter.is_empty() {
        char_strings(text)
    } else {
        text.split(delimiter).map(String::from).collect()
    };
    Ok(alloc_strings(rt, &parts))
}

pub(crate) fn baml_string_chars<R: Strings + Lists<ObjPtr>>(
    rt: &mut R,
    string: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let parts = char_strings(rt.str_text(string));
    Ok(alloc_strings(rt, &parts))
}

pub(crate) fn baml_string_lines<R: Strings + Lists<ObjPtr>>(
    rt: &mut R,
    string: ObjPtr,
) -> Result<ObjPtr, Fault> {
    let parts: Vec<String> = rt.str_text(string).lines().map(String::from).collect();
    Ok(alloc_strings(rt, &parts))
}

pub(crate) fn baml_string_slice<R: Strings>(
    rt: &mut R,
    string: ObjPtr,
    start: i64,
    end: i64,
) -> Result<ObjPtr, Fault> {
    // Codepoint-indexed, not byte-indexed; a negative index counts from the end.
    let range = slice_bounds(start, end, rt.str_char_len(string));
    let sliced: String = rt
        .str_text(string)
        .chars()
        .skip(range.start)
        .take(range.len())
        .collect();
    Ok(rt.str_alloc(&sliced))
}

pub(crate) fn baml_string_to_utf8<R: Strings>(rt: &mut R, string: ObjPtr) -> Result<ObjPtr, Fault> {
    let bytes = rt.str_text(string).as_bytes().to_vec();
    Ok(rt.bytes_alloc(&bytes))
}

pub(crate) fn baml_string_to_code_points<R: Strings + Lists<bex_vm_types::layout::RawWord>>(
    rt: &mut R,
    string: ObjPtr,
) -> Result<ObjPtr, Fault> {
    use bex_vm_types::layout::RawWord;
    let points: Vec<RawWord> = rt
        .str_text(string)
        .chars()
        .map(|ch| RawWord::from_int(i64::from(u32::from(ch))))
        .collect();
    Ok(rt.list_alloc(ListOf::Ints, &points))
}

pub(crate) fn baml_ops_equals_for_string_eq<R: Strings>(
    rt: &mut R,
    string: ObjPtr,
    other: ObjPtr,
) -> Result<bool, Fault> {
    Ok(rt.str_text(string) == rt.str_text(other))
}
