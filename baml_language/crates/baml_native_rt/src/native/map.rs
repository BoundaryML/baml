//! `baml.Map`.
//!
//! Every native here is generic: a key or a value is the one word a record
//! holds it in. How keys are hashed and compared, and whether values are
//! references, is known from the type ids of the call's type arguments.
//!
//! A map shared between tasks is locked for the length of each of these.
//! There is one task so far and no lock: `locked` is where it goes.

use super::{
    Arr, Elem, MapRef, NativeBamlMap, Natives, Root, Scope, Slot, TypeId, View, array_size,
};
use crate::map::{self, KeyKind};

fn kind(types: [TypeId; 2]) -> KeyKind {
    KeyKind::of(types[0].0 as usize)
}

/// Run `body` with the map at `map` held against other tasks.
fn locked<R>(_map: usize, body: impl FnOnce() -> R) -> R {
    body()
}

/// The keys or the values of `map`, in the order the keys were first set.
fn column<'s>(
    scope: &mut Scope<'s>,
    map: Root<'s, super::Map>,
    holds_refs: bool,
    values: bool,
) -> Root<'s, Arr<Slot>> {
    // SAFETY: a live map.
    let length = unsafe { map::len(map.raw()) };
    let reservation = scope.reserve(array_size(length));
    let array = reservation.slots(length, holds_refs);
    let mut next = 0;
    locked(map.raw(), || {
        // SAFETY: a live map, and nothing allocates inside a reservation.
        unsafe {
            map::for_each(map.raw(), |key, value| {
                array.set(next, if values { value } else { key });
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
            map::get(map.raw(), kind(types), key).is_some()
        })
    }

    fn get(types: [TypeId; 2], map: MapRef<'_>, key: usize) -> Option<usize> {
        locked(map.raw(), || {
            // SAFETY: a live map of the named key type and a key of it;
            // the value is read at once.
            unsafe { map::get(map.raw(), kind(types), key).map(|slot| crate::heap::load(slot)) }
        })
    }

    fn delete(types: [TypeId; 2], map: MapRef<'_>, key: usize) -> Option<usize> {
        // SAFETY: a live map of the named key type and a key of it.
        locked(map.raw(), || unsafe {
            map::delete(map.raw(), kind(types), key)
        })
    }

    fn clear(_types: [TypeId; 2], map: MapRef<'_>) {
        // SAFETY: a live map.
        locked(map.raw(), || unsafe { map::clear(map.raw()) });
    }

    fn set<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        map: Root<'s, super::Map>,
        key: Elem<'s>,
        value: Elem<'s>,
    ) -> Option<usize> {
        let (map, key, is_ref, value) =
            (map.raw(), key.get(scope), value.is_ref(), value.get(scope));
        // SAFETY: a live map of the named types, and a key and a value of
        // them.
        locked(map, || unsafe {
            map::set(scope.mutator(), map, kind(types), is_ref, key, value)
        })
    }

    fn get_or_insert<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        map: Root<'s, super::Map>,
        key: Elem<'s>,
        default: Elem<'s>,
    ) -> usize {
        let (kind, is_ref) = (kind(types), default.is_ref());
        let key = kind.canonical(key.get(scope));
        locked(map.raw(), || {
            // SAFETY: a live map of the named types, and a key and a value
            // of them.
            unsafe {
                if let Some(slot) = map::get(map.raw(), kind, key) {
                    return crate::heap::load(slot);
                }
                let hash = kind.hash(key);
                let value = default.get(scope);
                map::insert_new(scope.mutator(), map.raw(), kind, is_ref, key, hash, value);
            }
            default.get(scope)
        })
    }

    fn keys<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        map: Root<'s, super::Map>,
    ) -> Root<'s, Arr<Slot>> {
        column(scope, map, kind(types) == KeyKind::Str, false)
    }

    fn values<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 2],
        map: Root<'s, super::Map>,
    ) -> Root<'s, Arr<Slot>> {
        let holds_refs = scope.is_reference(types[1]);
        column(scope, map, holds_refs, true)
    }
}
