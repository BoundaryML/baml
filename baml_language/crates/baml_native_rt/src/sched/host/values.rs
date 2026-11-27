//! Reading the objects a host holds, and building the ones it passes in.
//!
//! Every function that takes a handle checks it: in range, live, and of
//! the kind it wants. A wrong one is refused with the function's error
//! value (zero for a handle, -1 for a count or a status), and
//! `baml_lib_last_error` says why. None consumes a handle: the host frees
//! every handle it was given.
//!
//! Reading never allocates on the heap. Building allocates once, after
//! every input has been checked, and turns handles into addresses only
//! after that allocation: a collection may move every object, and the
//! handles are where the collector leaves their new places.
//!
//! An array or a map is built whole and never changed by the host after.
//! It is accepted as an input once, right after it is made
//! ([`table::FRESH`]), so no container lands in two typed places.

use super::{HANDLE_BIT, host, items, refuse, table};
use crate::{
    heap::{Value, load},
    layout::{
        ARRAY_LENGTH, ARRAY_PTR, INSTANCE_CLASS, STORAGE_ELEMENTS, STRING_TAGS_END, TAG_ARRAY,
        TAG_ENUM_VARIANT, TAG_INSTANCE, TAG_MAP, TAG_OMITTED, TAG_STRING, TYPE_ID_MASK, VALUE,
        VARIANT_ENUM, VARIANT_INDEX, is_pointer,
    },
};

/// What the host sees of `value`: itself when it is a scalar, its tag
/// with [`HANDLE_BIT`] and a new handle when it is an object.
pub(super) fn crossing(value: Value) -> Value {
    if is_pointer(value.tag) {
        Value {
            tag: value.tag | HANDLE_BIT,
            payload: table::hold(value, 0),
        }
    } else {
        value
    }
}

/// Whether `tag`, the tag of an object, and `other`, a tag the host gave,
/// say the same type. A string's tag says whether it is a slice, which a
/// collection may change (a slice whose parent died becomes a string), so
/// any string tag stands for any string.
fn same_type(tag: usize, other: usize) -> bool {
    let is_string = |tag: usize| (TAG_STRING..STRING_TAGS_END).contains(&tag);
    tag == other || (is_string(tag) && is_string(other))
}

/// The value the host passed: a handle's object, or the scalar itself. A
/// scalar's tag with any bit beyond its type is refused (a slot must never
/// hold the busy bit or `THROWN`), and so is `OMITTED` unless `omitted`
/// says the place takes it.
pub(super) fn resolve(value: Value, omitted: bool) -> Result<Value, String> {
    if value.tag & HANDLE_BIT == 0 {
        if value.tag & !TYPE_ID_MASK != 0 || is_pointer(value.tag) {
            return Err(format!("a scalar with tag {:#x}", value.tag));
        }
        if value.tag == TAG_OMITTED && !omitted {
            return Err("OMITTED where no default is declared".to_owned());
        }
        return Ok(value);
    }
    let tag = value.tag & !HANDLE_BIT;
    if tag & !TYPE_ID_MASK != 0 {
        return Err(format!("a handle with tag {:#x}", value.tag));
    }
    let held = table::get(value.payload as u64)
        .ok_or_else(|| format!("handle {} is not live", value.payload))?;
    if !same_type(held.tag, tag) {
        return Err(format!(
            "handle {} holds a value of tag {}, not {tag}",
            value.payload, held.tag
        ));
    }
    Ok(held)
}

/// Check the inputs of one call or build: each resolves, each array or map
/// handle is fresh and passed once. Returns the container handles, to be
/// marked used once the whole call is accepted.
fn containers(values: &[Value]) -> Result<Vec<u64>, String> {
    let mut seen = Vec::new();
    for value in values {
        if value.tag & HANDLE_BIT != 0 {
            let tag = value.tag & !HANDLE_BIT;
            if tag == TAG_ARRAY || tag == TAG_MAP {
                let handle = value.payload as u64;
                if table::get(handle).is_none() {
                    return Err(format!("handle {handle} is not live"));
                }
                if table::flags(handle) & table::FRESH == 0 || seen.contains(&handle) {
                    return Err(format!(
                        "an array or a map is passed once, right after it is made: handle {handle}"
                    ));
                }
                seen.push(handle);
            }
        }
    }
    Ok(seen)
}

/// Accept every value of a call or a build, now that all are checked.
fn mark_used(containers: &[u64]) {
    for handle in containers {
        table::set_flags(*handle, 0);
    }
}

/// Check the values a call passes against its parameters, and take its
/// containers. Allocates nothing.
pub(super) fn check_arguments(function: usize, values: &[Value]) -> Result<(), String> {
    // SAFETY: the caller is the library's entry point, with no reference
    // to the host's state live.
    let host = unsafe { host() };
    let params = &host.interface.functions[function].params;
    if values.len() != params.len() {
        return Err(format!(
            "{} arguments where {} are declared",
            values.len(),
            params.len()
        ));
    }
    let used = containers(values)?;
    for (place, (value, (ty, has_default))) in values.iter().zip(params).enumerate() {
        let resolved =
            resolve(*value, *has_default).map_err(|why| format!("argument {place}: {why}"))?;
        // SAFETY: a live value, and nothing allocates here.
        if resolved.tag != TAG_OMITTED && !unsafe { host.interface.fits(resolved, *ty) } {
            return Err(format!("argument {place} is not of its parameter's type"));
        }
    }
    mark_used(&used);
    Ok(())
}

/// The object of `handle` if its tag is `tag`.
fn object(handle: u64, tag: usize) -> Result<usize, String> {
    let value = table::get(handle).ok_or_else(|| format!("handle {handle} is not live"))?;
    if value.tag != tag {
        return Err(format!(
            "handle {handle} holds a value of tag {}, not {tag}",
            value.tag
        ));
    }
    Ok(value.payload)
}

/// Write `value` to `out`, or say why there is none.
///
/// # Safety
///
/// `out` is writable.
unsafe fn answer(out: *mut Value, value: Result<Value, String>) -> i32 {
    match value {
        Ok(value) => {
            // SAFETY: the caller's contract.
            unsafe { out.write(value) };
            0
        }
        Err(why) => refuse(why, -1),
    }
}

/// The number, in the library's table of exports, of the class of the
/// instance `handle` holds, or of the enum of the variant it holds; -1 for
/// anything else, or a class the library does not export.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_class_of(handle: u64) -> i64 {
    let Some(value) = table::get(handle) else {
        return refuse(format!("handle {handle} is not live"), -1);
    };
    // SAFETY: an instance has its class object, a variant its enum's.
    let class = unsafe {
        match value.tag {
            TAG_INSTANCE => load(value.payload + INSTANCE_CLASS),
            TAG_ENUM_VARIANT => load(value.payload + VARIANT_ENUM),
            _ => return refuse("not an instance or an enum value".to_owned(), -1),
        }
    };
    // SAFETY: as above.
    let host = unsafe { host() };
    host.interface.by_class.get(&class).map_or_else(
        || refuse("a class the library does not export".to_owned(), -1),
        |number| i64::try_from(*number).unwrap_or(-1),
    )
}

/// Field `field` (in declaration order) of the instance `handle` holds,
/// written to `out`: a scalar, or a new handle. Returns zero, or -1.
///
/// # Safety
///
/// `out` is writable. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_field_get(handle: u64, field: u32, out: *mut Value) -> i32 {
    let value = (|| {
        let instance = object(handle, TAG_INSTANCE)?;
        // SAFETY: as above; an instance has its class object.
        let host = unsafe { host() };
        let class = unsafe { load(instance + INSTANCE_CLASS) };
        let number = *host
            .interface
            .by_class
            .get(&class)
            .ok_or("an instance of a class the library does not export")?;
        let field = host.interface.exported[number]
            .fields
            .get(field as usize)
            .ok_or_else(|| format!("no field {field}"))?;
        // SAFETY: a slot of the instance.
        Ok(crossing(unsafe {
            Value::unshared(instance + field.offset)
        }))
    })();
    // SAFETY: the caller's contract.
    unsafe { answer(out, value) }
}

/// How many elements the array `handle` holds has, or -1.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_array_len(handle: u64) -> i64 {
    match object(handle, TAG_ARRAY) {
        // SAFETY: a live array.
        Ok(array) => i64::try_from(unsafe { load(array + ARRAY_LENGTH) }).unwrap_or(-1),
        Err(why) => refuse(why, -1),
    }
}

/// Element `index` of the array `handle` holds, written to `out`.
/// Returns zero, or -1.
///
/// # Safety
///
/// `out` is writable. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_array_get(handle: u64, index: u64, out: *mut Value) -> i32 {
    let value = object(handle, TAG_ARRAY).and_then(|array| {
        // SAFETY: a live array, whose storage holds its length of values.
        unsafe {
            let length = load(array + ARRAY_LENGTH) as u64;
            if index >= length {
                return Err(format!("index {index} of an array of {length}"));
            }
            let index = usize::try_from(index).expect("below a length");
            let slot = load(array + ARRAY_PTR) + STORAGE_ELEMENTS + index * VALUE;
            Ok(crossing(Value::unshared(slot)))
        }
    });
    // SAFETY: the caller's contract.
    unsafe { answer(out, value) }
}

/// How many keys the map `handle` holds has, or -1.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_map_len(handle: u64) -> i64 {
    match object(handle, TAG_MAP) {
        // SAFETY: a live map.
        Ok(map) => i64::try_from(unsafe { crate::map::len(map) }).unwrap_or(-1),
        Err(why) => refuse(why, -1),
    }
}

/// The first entry of the map `handle` holds at or after `cursor`, in the
/// order the keys were first set: its key and value written to `key` and
/// `value`. Returns the cursor of the next entry, -1 when there is none
/// left, or -2 for a wrong handle. Start with zero.
///
/// # Safety
///
/// Both are writable. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_map_next(
    handle: u64,
    cursor: u64,
    key: *mut Value,
    value: *mut Value,
) -> i64 {
    let map = match object(handle, TAG_MAP) {
        Ok(map) => map,
        Err(why) => return refuse(why, -2),
    };
    // SAFETY: a live map, and the caller's contract.
    unsafe {
        match crate::map::entry_from(map, usize::try_from(cursor).unwrap_or(usize::MAX)) {
            Some((number, found_key, found_value)) => {
                key.write(crossing(found_key));
                value.write(crossing(found_value));
                i64::try_from(number + 1).unwrap_or(-2)
            }
            None => -1,
        }
    }
}

/// The position in its enum of the variant `handle` holds, or -1.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_variant_index(handle: u64) -> i64 {
    match object(handle, TAG_ENUM_VARIANT) {
        // SAFETY: a variant's object.
        Ok(variant) => i64::try_from(unsafe { load(variant + VARIANT_INDEX) }).unwrap_or(-1),
        Err(why) => refuse(why, -1),
    }
}

/// Variant `index` of the enum numbered `enumeration` in the table of
/// exports, written to `out` as a handle. Returns zero, or -1.
///
/// # Safety
///
/// `out` is writable. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_variant(enumeration: u32, index: u32, out: *mut Value) -> i32 {
    let value = (|| {
        // SAFETY: the caller's contract.
        let host = unsafe { host() };
        let exported = host
            .interface
            .exported
            .get(enumeration as usize)
            .filter(|exported| exported.is_enum)
            .ok_or_else(|| format!("{enumeration} is not an exported enum"))?;
        if index as usize >= exported.variants {
            return Err(format!("no variant {index}"));
        }
        // SAFETY: an enum's class object lists its variants.
        let variant = unsafe { crate::class::variant(exported.class, index as usize) };
        Ok(crossing(Value {
            tag: TAG_ENUM_VARIANT,
            payload: variant,
        }))
    })();
    // SAFETY: the caller's contract.
    unsafe { answer(out, value) }
}

/// A new instance of the class numbered `class` in the table of exports,
/// its fields the `count` values at `fields` in declaration order. Each is
/// checked against its field's type; on a refusal `bad_field`, when not
/// null, is set to the field's position (or -1). Returns the instance's
/// handle, or zero. May collect.
///
/// # Safety
///
/// `fields` is `count` readable values (null when `count` is zero), and
/// `bad_field` null or writable. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_instance_new(
    class: u32,
    fields: *const Value,
    count: usize,
    bad_field: *mut i32,
) -> u64 {
    // SAFETY: the caller's contract.
    let values = unsafe { items(fields, count) };
    let blame = |field: i32| {
        if !bad_field.is_null() {
            // SAFETY: the caller's contract.
            unsafe { bad_field.write(field) };
        }
    };
    blame(-1);
    // Check everything first: nothing here allocates.
    let object_class = {
        // SAFETY: as above.
        let host = unsafe { host() };
        let Some(exported) = host
            .interface
            .exported
            .get(class as usize)
            .filter(|exported| !exported.is_enum)
        else {
            return refuse(format!("{class} is not an exported class"), 0);
        };
        if values.len() != exported.fields.len() {
            return refuse(
                format!(
                    "{} fields where the class has {}",
                    values.len(),
                    exported.fields.len()
                ),
                0,
            );
        }
        let used = match containers(values) {
            Ok(used) => used,
            Err(why) => return refuse(why, 0),
        };
        for (place, (value, field)) in values.iter().zip(&exported.fields).enumerate() {
            let resolved = match resolve(*value, false) {
                Ok(resolved) => resolved,
                Err(why) => {
                    blame(i32::try_from(place).unwrap_or(-1));
                    return refuse(format!("field {place}: {why}"), 0);
                }
            };
            // SAFETY: a live value, and nothing allocates here.
            if !unsafe { host.interface.fits(resolved, field.ty) } {
                blame(i32::try_from(place).unwrap_or(-1));
                return refuse(format!("field {place} is not of its declared type"), 0);
            }
        }
        mark_used(&used);
        exported.class
    };
    // The one allocation.
    // SAFETY: as above.
    let instance = unsafe {
        let worker = super::host().worker();
        worker.clear_anchor();
        let instance = worker.allocate(crate::class::size(object_class));
        crate::class::format(instance, object_class);
        instance
    };
    // Now the handles say where the fields' objects are.
    // SAFETY: as above.
    let host = unsafe { host() };
    let exported = &host.interface.exported[class as usize];
    for (value, field) in values.iter().zip(&exported.fields) {
        let resolved = resolve(*value, false).expect("checked before the allocation");
        // SAFETY: a slot of the fresh instance.
        unsafe { Value::init(instance + field.offset, resolved) };
    }
    table::hold(
        Value {
            tag: TAG_INSTANCE,
            payload: instance,
        },
        0,
    ) as u64
}

/// A new array of the `count` values at `values`. Its handle can be
/// passed once, where an array is wanted, and is checked there. Returns
/// zero when a value is refused. May collect.
///
/// # Safety
///
/// `values` is `count` readable values (null when `count` is zero). Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_array_new(values: *const Value, count: usize) -> u64 {
    // SAFETY: the caller's contract.
    let values = unsafe { items(values, count) };
    let used = match containers(values).and_then(|used| {
        values
            .iter()
            .enumerate()
            .try_for_each(|(place, value)| {
                resolve(*value, false)
                    .map(drop)
                    .map_err(|why| format!("element {place}: {why}"))
            })
            .map(|()| used)
    }) {
        Ok(used) => used,
        Err(why) => return refuse(why, 0),
    };
    mark_used(&used);
    // SAFETY: a worker with no task running.
    let array = unsafe {
        let worker = super::host().worker();
        worker.clear_anchor();
        crate::array::array_new(worker, count)
    };
    if count > 0 {
        // SAFETY: an array of `count` elements has storage for them, and
        // nothing allocates from here on.
        unsafe {
            let elements = load(array + ARRAY_PTR) + STORAGE_ELEMENTS;
            for (place, value) in values.iter().enumerate() {
                let resolved = resolve(*value, false).expect("checked before the allocation");
                Value::init(elements + place * VALUE, resolved);
            }
        }
    }
    table::hold(
        Value {
            tag: TAG_ARRAY,
            payload: array,
        },
        table::FRESH,
    ) as u64
}

/// A new map of the `count` keys at `keys`, each a handle of a string the
/// host made (not a part of one), to the values at `values`. A key given
/// twice keeps the later value. Its handle can be passed once, where a
/// map is wanted, and is checked there. Returns zero when a key or a value
/// is refused. May collect.
///
/// # Safety
///
/// Both are `count` readable values (null when `count` is zero). Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_map_new(
    keys: *const Value,
    values: *const Value,
    count: usize,
) -> u64 {
    // SAFETY: the caller's contract.
    let (keys, values) = unsafe { (items(keys, count), items(values, count)) };
    let both: Vec<Value> = keys.iter().chain(values).copied().collect();
    let checked = containers(&both).and_then(|used| {
        for (place, key) in keys.iter().enumerate() {
            let resolved = resolve(*key, false).map_err(|why| format!("key {place}: {why}"))?;
            if resolved.tag != TAG_STRING {
                return Err(format!("key {place} is not a string the host made"));
            }
        }
        for (place, value) in values.iter().enumerate() {
            resolve(*value, false).map_err(|why| format!("value {place}: {why}"))?;
        }
        Ok(used)
    });
    let used = match checked {
        Ok(used) => used,
        Err(why) => return refuse(why, 0),
    };
    mark_used(&used);
    // SAFETY: a worker with no task running.
    let map = unsafe {
        let worker = super::host().worker();
        worker.clear_anchor();
        crate::map::new(worker, count)
    };
    for (key, value) in keys.iter().zip(values) {
        let key = resolve(*key, false).expect("checked before the allocation");
        let value = resolve(*value, false).expect("checked before the allocation");
        // SAFETY: a map with room for `count` keys, a string that holds its
        // own text, and nothing allocates from here on.
        unsafe { crate::map::set_in_place(map, key.payload, value) };
    }
    table::hold(
        Value {
            tag: TAG_MAP,
            payload: map,
        },
        table::FRESH,
    ) as u64
}
