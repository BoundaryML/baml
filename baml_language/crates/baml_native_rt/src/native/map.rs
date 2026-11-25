//! `baml.Map`.
//!
//! Every native here is generic: a key or a value is the slot a record
//! holds it in, and one passed on its own is a value too. How keys are
//! hashed and compared is said by a key's tag.
//!
//! A map shared between tasks is locked for the length of each of these.
//! There is one task so far and no lock: `locked` is where it goes.

use super::{Arr, Elem, MapRef, NativeBamlMap, Natives, Root, Scope, Slot, View, array_size};
use crate::{heap::Value, map};

/// Run `body` with the map at `map` held against other tasks.
fn locked<R>(_map: usize, body: impl FnOnce() -> R) -> R {
    body()
}

/// The keys or the values of `map`, in the order the keys were first set.
fn column<'s>(
    scope: &mut Scope<'s>,
    map: Root<'s, super::Map>,
    values: bool,
) -> Root<'s, Arr<Slot>> {
    // SAFETY: a live map.
    let length = unsafe { map::len(map.raw(scope)) };
    let reservation = scope.reserve(array_size(length));
    let array = reservation.array::<Slot>(length);
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
    fn length(map: MapRef<'_>) -> i64 {
        // SAFETY: a live map.
        let length = unsafe { map::len(map.raw()) };
        i64::try_from(length).unwrap_or(i64::MAX)
    }

    fn has(map: MapRef<'_>, key: Value) -> bool {
        // SAFETY: a live map and a key of its key type.
        locked(map.raw(), || unsafe { map::get(map.raw(), key).is_some() })
    }

    fn get(map: MapRef<'_>, key: Value) -> Option<Value> {
        locked(map.raw(), || {
            // SAFETY: a live map and a key of its key type; the value is
            // read at once.
            unsafe { map::get(map.raw(), key).map(|slot| Value::load(slot)) }
        })
    }

    fn delete(map: MapRef<'_>, key: Value) -> Option<Value> {
        // SAFETY: a live map and a key of its key type.
        locked(map.raw(), || unsafe { map::delete(map.raw(), key) })
    }

    fn clear(map: MapRef<'_>) {
        // SAFETY: a live map.
        locked(map.raw(), || unsafe { map::clear(map.raw()) });
    }

    fn set<'s>(
        scope: &mut Scope<'s>,
        map: Root<'s, super::Map>,
        key: Elem<'s>,
        value: Elem<'s>,
    ) -> Option<Value> {
        let (map, key, value) = (map.raw(scope), key.value(scope), value.value(scope));
        // SAFETY: a live map, and a key and a value of its types.
        locked(map, || unsafe {
            map::set(scope.mutator(), map, key, value)
        })
    }

    fn get_or_insert<'s>(
        scope: &mut Scope<'s>,
        map: Root<'s, super::Map>,
        key: Elem<'s>,
        default: Elem<'s>,
    ) -> Value {
        locked(map.raw(scope), || {
            // SAFETY: a live map, and a key and a value of its types.
            unsafe {
                if let Some(slot) = map::get(map.raw(scope), key.value(scope)) {
                    return Value::load(slot);
                }
                let (map, key, value) = (map.raw(scope), key.value(scope), default.value(scope));
                map::set(scope.mutator(), map, key, value);
            }
            default.value(scope)
        })
    }

    fn keys<'s>(scope: &mut Scope<'s>, map: Root<'s, super::Map>) -> Root<'s, Arr<Slot>> {
        column(scope, map, false)
    }

    fn values<'s>(scope: &mut Scope<'s>, map: Root<'s, super::Map>) -> Root<'s, Arr<Slot>> {
        column(scope, map, true)
    }
}
