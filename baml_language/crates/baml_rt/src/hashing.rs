//! `baml.hash.DefaultHasher` and `baml._hash_structural_default`.
//!
//! The structural walk frames every value exactly as the interpreter's
//! `HashDriver` does and feeds each frame to the hasher state through the
//! compiler-supplied `write` dispatch, so a user hasher observes the same
//! byte chunks in the same order.
use crate::{Dyn, FLOAT_TAG, R, telemetry::capture::Shape};
use std::{cell::RefCell, collections::HashSet, hash::Hasher as _};

type State = RefCell<std::collections::hash_map::DefaultHasher>;
const OPAQUE_DESC: u32 = 18;

/// The opaque `_state` of a new `DefaultHasher`.
pub fn new_state() -> R<usize> {
    Ok(crate::opaque::Opaque::new(OPAQUE_DESC, State::default())?.as_raw())
}

/// # Safety
/// `state` is a live `DefaultHasher` state allocation.
pub unsafe fn write(state: usize, bytes: &[u8]) {
    unsafe { crate::opaque::Opaque::<State>::from_raw(state) }
        .with(|h| h.borrow_mut().write(bytes));
}

/// # Safety
/// `state` is a live `DefaultHasher` state allocation.
pub unsafe fn finish_raw(state: usize) -> u64 {
    unsafe { crate::opaque::Opaque::<State>::from_raw(state) }.with(|h| h.borrow().finish())
}

/// `DefaultHasher.finish()`: the interpreter shifts the 64-bit digest into
/// BAML's 63-bit `int` range.
/// # Safety
/// `state` is a live `DefaultHasher` state allocation.
pub unsafe fn finish(state: usize) -> i64 {
    (unsafe { finish_raw(state) }).cast_signed() >> 1
}

fn framed(tag: u8, bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![tag];
    result.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    result.extend_from_slice(bytes);
    result
}

fn panic(message: &str) -> Box<crate::Error> {
    crate::language::from_panic(bex_vm_types::errors::VmPanic::UserPanic {
        message: message.to_owned(),
    })
}

/// Program hooks for the walk: write a frame into a hasher state, and create
/// the fresh per-entry `DefaultHasher` (value, opaque state) maps hash with.
pub trait Hooks {
    fn write(&mut self, state: Dyn, bytes: &[u8]) -> R<()>;
    fn new_hasher(&mut self) -> R<(Dyn, usize)>;
}

enum Work {
    Value(Dyn),
    Exit(u64),
    MapEntry(Dyn, Dyn),
    FinishMapEntry(Dyn, usize),
    FinishMap,
}

/// # Safety
/// `value` and `state` are live values of the active program described by
/// `shapes`. Hooks may allocate; every pending value is rooted while they run.
pub unsafe fn structural(value: Dyn, state: Dyn, shapes: &[Shape], hooks: &mut dyn Hooks) -> R<()> {
    let mut state = state;
    let mut pending = vec![Work::Value(value)];
    let mut active = HashSet::new();
    let mut map_hashes: Vec<Vec<u64>> = Vec::new();
    let roots = crate::ROOTS.get();
    let mut leases = Vec::new();
    let pointer = |v: Dyn| {
        v.tag >= 3
            && v.tag != FLOAT_TAG
            && v.tag != u64::MAX
            && v.payload != 0
            && !matches!(
                shapes.get((v.tag - 3) as usize),
                Some(Shape::Enum { .. }) | None
            )
    };
    while let Some(work) = pending.pop() {
        let bytes = match work {
            Work::Exit(ptr) => {
                active.remove(&ptr);
                continue;
            }
            Work::MapEntry(key, value) => {
                let (fresh, handle) = hooks.new_hasher()?;
                if pointer(fresh) {
                    leases.push(unsafe { (&*roots).root(fresh.payload as usize) });
                }
                let previous = std::mem::replace(&mut state, fresh);
                pending.push(Work::FinishMapEntry(previous, handle));
                pending.push(Work::Value(value));
                pending.push(Work::Value(key));
                continue;
            }
            Work::FinishMapEntry(previous, handle) => {
                state = previous;
                map_hashes
                    .last_mut()
                    .expect("map entry belongs to an active map")
                    .push(unsafe { finish_raw(handle) });
                continue;
            }
            Work::FinishMap => {
                let mut hashes = map_hashes.pop().expect("active map hash frame");
                hashes.sort_unstable();
                let mut bytes = Vec::with_capacity(hashes.len() * 8);
                for hash in hashes {
                    bytes.extend_from_slice(&hash.to_le_bytes());
                }
                framed(8, &bytes)
            }
            Work::Value(value) => {
                let value = if value.tag >= 3
                    && value.tag != FLOAT_TAG
                    && value.tag != u64::MAX
                    && value.payload == 0
                    && !matches!(
                        shapes.get((value.tag - 3) as usize),
                        Some(Shape::Enum { .. })
                    ) {
                    Dyn { tag: 2, payload: 0 }
                } else {
                    value
                };
                if pointer(value) {
                    if !active.insert(value.payload) {
                        return Err(panic("Cannot hash a cyclic value"));
                    }
                    pending.push(Work::Exit(value.payload));
                }
                match value.tag {
                    2 => vec![0],
                    0 => framed(1, &(value.payload as i64).to_le_bytes()),
                    1 => vec![2, u8::from(value.payload != 0)],
                    u64::MAX => return Err(panic("Cannot hash an omitted argument")),
                    FLOAT_TAG => {
                        let value = f64::from_bits(value.payload);
                        let bits = if value.is_nan() {
                            f64::NAN.to_bits()
                        } else if value == 0.0 {
                            0
                        } else {
                            value.to_bits()
                        };
                        framed(3, &bits.to_le_bytes())
                    }
                    tag => match shapes.get((tag - 3) as usize) {
                        Some(Shape::String) => framed(
                            5,
                            unsafe { crate::strings::Str::from_raw(value.payload as usize) }
                                .to_owned()
                                .as_bytes(),
                        ),
                        Some(Shape::Bigint) => framed(
                            4,
                            &unsafe { crate::native::bigint_owned(value.payload as usize) }
                                .to_signed_bytes_le(),
                        ),
                        Some(Shape::Uint8Array) => {
                            framed(6, unsafe { crate::bytes::data(value.payload as usize) })
                        }
                        Some(Shape::List { .. }) => {
                            let values = unsafe { crate::format::children(shapes, value) };
                            let bytes = framed(7, &(values.len() as u64).to_le_bytes());
                            pending.extend(values.into_iter().rev().map(Work::Value));
                            bytes
                        }
                        Some(
                            Shape::Map { .. } | Shape::MapStringInt | Shape::MapStringPtr { .. },
                        ) => {
                            let values = unsafe { crate::format::children(shapes, value) };
                            map_hashes.push(Vec::with_capacity(values.len() / 2));
                            pending.push(Work::FinishMap);
                            for pair in values.chunks_exact(2).rev() {
                                pending.push(Work::MapEntry(pair[0], pair[1]));
                            }
                            continue;
                        }
                        Some(
                            Shape::Class {
                                declaration,
                                fields,
                            }
                            | Shape::GenericClass {
                                declaration,
                                fields,
                                ..
                            },
                        ) => {
                            let mut bytes = framed(9, &declaration().tag.as_i64().to_le_bytes());
                            bytes.extend_from_slice(&(fields.len() as u64).to_le_bytes());
                            let values = unsafe { crate::format::children(shapes, value) };
                            pending.extend(values.into_iter().rev().map(Work::Value));
                            bytes
                        }
                        Some(Shape::Enum { declaration, .. }) => {
                            let mut bytes = framed(10, &declaration().tag.as_i64().to_le_bytes());
                            bytes.extend_from_slice(&value.payload.to_le_bytes());
                            bytes
                        }
                        _ => return Err(panic("Value does not support structural hashing")),
                    },
                }
            }
        };
        // User code may run in `write`; keep every pending operand rooted.
        for work in &pending {
            if let Work::Value(v) | Work::MapEntry(v, _) = work
                && pointer(*v)
            {
                leases.push(unsafe { (&*roots).root(v.payload as usize) });
            }
            if let Work::MapEntry(_, v) = work
                && pointer(*v)
            {
                leases.push(unsafe { (&*roots).root(v.payload as usize) });
            }
        }
        hooks.write(state, &bytes)?;
    }
    Ok(())
}
