//! `baml.Map`.
//!
//! Every native here is generic: a key or a value is the slot a record
//! holds it in. How keys are hashed and compared is said by a key's tag,
//! and what tag a key or a value passed on its own has by the type ids of
//! the call's type arguments.
//!
//! A map shared between tasks is locked for the length of each of these.
//! There is one task so far and no lock: `locked` is where it goes.

use super::{
    Arr, Elem, MapRef, NativeBamlMap, Natives, Root, Scope, Slot, TypeId, View, array_size,
};
use crate::{
    heap::Value,
    layout::{TAG_BOX_FLOAT, TAG_FLOAT, TAG_INT, TAG_STRING},
    map,
};

/// The key to search a map for, given as compiled code holds it. A search
/// reads of a key's tag only whether it is a string, a `float` or anything
/// else, which is its bits, so every key of the last kind is given the tag
/// of an `int`.
///
/// # Safety
///
/// `key` is a key of the type the call names: a live string when that is
/// `string`.
unsafe fn sought(types: [TypeId; 2], key: usize) -> Value {
    match types[0].0 as usize {
        // SAFETY: the caller's contract.
        TAG_STRING => unsafe { Value::reference(key) },
        TAG_BOX_FLOAT => Value::immediate(TAG_FLOAT, key),
        _ => Value::immediate(TAG_INT, key),
    }
}

/// Run `body` with the map at `map` held against other tasks.
fn locked<R>(_map: usize, body: impl FnOnce() -> R) -> R {
    body()
}

/// The keys or the values of `map`, in the order the keys were first set.
fn column<'s>(
    scope: &mut Scope<'s>,
    map: Root<'s, super::Map>,
    element: TypeId,
    values: bool,
) -> Root<'s, Arr<Slot>> {
    // SAFETY: a live map.
    let length = unsafe { map::len(map.raw(scope)) };
    let reservation = scope.reserve(array_size(length));
    let array = reservation.elements(length, element);
    let mut next = 0;
    // Read after the reservation: the map may have moved.
    let map = map.raw(reservation.scope);
    locked(map, || {
        // SAFETY: a live map, and nothing allocates inside a reservation.
        unsafe {
            map::for_each(map, |key, value| {
                array.set_value(next, if values { value } else { key });
                next += 1;
            });
        }
    });
    let array = array.keep();
    reservation.finish(array)
}

impl NativeBamlMap for Natives {
    fn length(_types: [TypeId; 2], map: MapRef<'_>) -> i64 {
        // SAFETY: a live map.
        let length = unsafe { map::len(map.raw()) };
        i64::try_from(length).unwrap_or(i64::MAX)
    }

    fn has(types: [TypeId; 2], map: MapRef<'_>, key: usize) -> bool {
        // SAFETY: a live map of the named key type and a key of it.
        locked(map.raw(), || unsafe {
            map::get(map.raw(), sought(types, key)).is_some()
        })
    }

    fn get(types: [TypeId; 2], map: MapRef<'_>, key: usize) -> Option<usize> {
        locked(map.raw(), || {
            // SAFETY: a live map of the named key type and a key of it;
            // the value is read at once.
            unsafe { map::get(map.raw(), sought(types, key)).map(|slot| Value::load(slot).payload) }
        })
    }

    fn delete(types: [TypeId; 2], map: MapRef<'_>, key: usize) -> Option<usize> {
        // SAFETY: a live map of the named key type and a key of it.
        locked(map.raw(), || unsafe {
            map::delete(map.raw(), sought(types, key)).map(|value| value.payload)
        })
    }

    fn clear(_types: [TypeId; 2], map: MapRef<'_>) {
        // SAFETY: a live map.
        locked(map.raw(), || unsafe { map::clear(map.raw()) });
    }

    fn set<'s>(
        scope: &mut Scope<'s>,
        _types: [TypeId; 2],
        map: Root<'s, super::Map>,
        key: Elem<'s>,
        value: Elem<'s>,
    ) -> Option<usize> {
        let (map, key, value) = (map.raw(scope), key.value(scope), value.value(scope));
        // SAFETY: a live map of the named types, and a key and a value of
        // them.
        locked(map, || unsafe {
            map::set(scope.mutator(), map, key, value).map(|before| before.payload)
        })
    }

    fn get_or_insert<'s>(
        scope: &mut Scope<'s>,
        _types: [TypeId; 2],
        map: Root<'s, super::Map>,
        key: Elem<'s>,
        default: Elem<'s>,
    ) -> usize {
        locked(map.raw(scope), || {
            // SAFETY: a live map of the named types, and a key and a value
            // of them.
            unsafe {
                if let Some(slot) = map::get(map.raw(scope), key.value(scope)) {
                    return Value::load(slot).payload;
                }
                let (map, key, value) = (map.raw(scope), key.value(scope), default.value(scope));
                map::set(scope.mutator(), map, key, value);
            }
            default.get(scope)
        })
    }

    fn keys<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        map: Root<'s, super::Map>,
    ) -> Root<'s, Arr<Slot>> {
        column(scope, map, types[0], false)
    }

    fn values<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        map: Root<'s, super::Map>,
    ) -> Root<'s, Arr<Slot>> {
        column(scope, map, types[1], true)
    }
}
