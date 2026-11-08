//! Structural map-key operations over the pack's precise layout metadata.
use std::{collections::hash_map::DefaultHasher, hash::Hasher};

use crate::{
    Dyn, R,
    strings::{Cx, Str},
    telemetry::capture::Shape,
};
use baml_rt_gc::SlotKind;

pub type Panic = fn(&mut Cx, &str) -> Box<crate::Error>;

fn framed(state: &mut DefaultHasher, tag: u8, bytes: &[u8]) {
    state.write(&[tag]);
    state.write(&(bytes.len() as u64).to_le_bytes());
    state.write(bytes);
}

/// # Safety
/// Every value must obey the supplied pack metadata and current heap lifetime.
pub unsafe fn hash(cx: &mut Cx, value: Dyn, shapes: &[Shape], panic: Panic) -> R<u64> {
    let mut state = DefaultHasher::new();
    unsafe {
        feed(cx, value, shapes, panic, &mut state, &mut Vec::new())?;
    }
    Ok(state.finish())
}

unsafe fn feed(
    cx: &mut Cx,
    value: Dyn,
    shapes: &[Shape],
    panic: Panic,
    state: &mut DefaultHasher,
    active: &mut Vec<usize>,
) -> R<()> {
    crate::check_stack()?;
    match value.tag {
        0 => {
            framed(state, 1, &value.payload.to_le_bytes());
            return Ok(());
        }
        1 => {
            state.write(&[2, u8::from(value.payload != 0)]);
            return Ok(());
        }
        2 => {
            state.write(&[0]);
            return Ok(());
        }
        crate::FLOAT_TAG => {
            let value = f64::from_bits(value.payload);
            let bits = if value.is_nan() {
                f64::NAN.to_bits()
            } else if value == 0.0 {
                0
            } else {
                value.to_bits()
            };
            framed(state, 3, &bits.to_le_bytes());
            return Ok(());
        }
        3 => {
            framed(
                state,
                5,
                unsafe { Str::from_raw(value.payload as usize) }
                    .as_str(cx)
                    .as_bytes(),
            );
            return Ok(());
        }
        _ => {}
    }
    let shape = &shapes[(value.tag - 3) as usize];
    if let Shape::Enum { declaration, .. } = shape {
        framed(state, 10, &declaration().tag.as_i64().to_le_bytes());
        state.write(&value.payload.to_le_bytes());
        return Ok(());
    }
    if value.payload == 0 {
        state.write(&[0]);
        return Ok(());
    }
    match shape {
        Shape::Bigint => {
            framed(
                state,
                4,
                &unsafe { crate::native::bigint_owned(value.payload as usize) }
                    .to_signed_bytes_le(),
            );
            return Ok(());
        }
        Shape::Bytes => {
            framed(state, 6, &unsafe {
                crate::native::bytes_owned(value.payload as usize)
            });
            return Ok(());
        }
        _ => {}
    }
    let pointer = value.payload as usize;
    if active.contains(&pointer) {
        return Err(panic(cx, "Cannot hash a cyclic value"));
    }
    active.push(pointer);
    match shape {
        Shape::Class {
            declaration,
            fields,
        }
        | Shape::GenericClass {
            declaration,
            fields,
            ..
        } => {
            framed(state, 9, &declaration().tag.as_i64().to_le_bytes());
            state.write(&(fields.len() as u64).to_le_bytes());
            for field in *fields {
                let child = unsafe { field_value(value, field) };
                unsafe {
                    feed(cx, child, shapes, panic, state, active)?;
                }
            }
        }
        Shape::List { .. } => {
            let len = unsafe { list_len(value, shape) };
            framed(state, 7, &(len as u64).to_le_bytes());
            for i in 0..len {
                unsafe {
                    feed(
                        cx,
                        list_value(value, shape, i),
                        shapes,
                        panic,
                        state,
                        active,
                    )?;
                }
            }
        }
        Shape::Map { len, entry, .. } => {
            let mut hashes = Vec::new();
            for i in 0..unsafe { len(pointer) } {
                let (key, child) = unsafe { entry(pointer, i) };
                let mut item = DefaultHasher::new();
                unsafe {
                    feed(cx, key, shapes, panic, &mut item, active)?;
                    feed(cx, child, shapes, panic, &mut item, active)?;
                }
                hashes.push(item.finish());
            }
            hashes.sort_unstable();
            state.write(&[8]);
            state.write(&((hashes.len() * 8) as u64).to_le_bytes());
            for hash in hashes {
                state.write(&hash.to_le_bytes());
            }
        }
        Shape::MapStringInt => {
            let map = unsafe { super::Map::<Str, i64>::from_raw(pointer) };
            let mut hashes = Vec::new();
            for i in 0..map.len() {
                let (key, child) = map.entry(i).unwrap();
                let mut item = DefaultHasher::new();
                framed(&mut item, 5, key.as_str(cx).as_bytes());
                framed(&mut item, 1, &child.to_le_bytes());
                hashes.push(item.finish());
            }
            hashes.sort_unstable();
            state.write(&[8]);
            state.write(&((hashes.len() * 8) as u64).to_le_bytes());
            for hash in hashes {
                state.write(&hash.to_le_bytes());
            }
        }
        _ => return Err(panic(cx, "Value does not support structural hashing")),
    }
    active.pop();
    Ok(())
}

pub(super) unsafe fn field_value(value: Dyn, field: &baml_rt_gc::Field) -> Dyn {
    let slot = unsafe { (value.payload as *const u8).add(field.offset) };
    match field.slot {
        SlotKind::Tagged => unsafe { slot.cast::<Dyn>().read() },
        _ => {
            let payload = unsafe { slot.cast::<u64>().read() };
            Dyn {
                tag: if matches!(field.slot, SlotKind::Pointer) && payload == 0 {
                    2
                } else {
                    u64::from(field.type_id)
                },
                payload,
            }
        }
    }
}
pub(super) unsafe fn list_len(value: Dyn, shape: &Shape) -> usize {
    let Shape::List { growing, .. } = shape else {
        unreachable!()
    };
    unsafe {
        (value.payload as *const usize)
            .add(if *growing { 2 } else { 1 })
            .read()
    }
}
unsafe fn list_value(value: Dyn, shape: &Shape, i: usize) -> Dyn {
    let Shape::List {
        growing,
        tag,
        element,
    } = shape
    else {
        unreachable!()
    };
    let object = value.payload as *const usize;
    let data = if *growing {
        (unsafe { object.add(1).read() }) as *const usize
    } else {
        object
    };
    if *tag == u64::MAX - 1 {
        return unsafe { data.add(2).cast::<Dyn>().add(i).read() };
    }
    let payload = unsafe { data.add(2 + i).read() as u64 };
    Dyn {
        tag: if *tag >= 3
            && *tag != crate::FLOAT_TAG
            && payload == 0
            && !matches!(
                element,
                crate::telemetry::capture::Type::Enum(_)
                    | crate::telemetry::capture::Type::EnumVariant(..)
            ) {
            2
        } else {
            *tag
        },
        payload,
    }
}

/// # Safety
/// Values must obey the supplied metadata and remain in the current instance.
pub unsafe fn equals(cx: &mut Cx, left: Dyn, right: Dyn, shapes: &[Shape]) -> R<bool> {
    unsafe { equal(cx, left, right, shapes, &mut Vec::new()) }
}
unsafe fn equal(
    cx: &mut Cx,
    left: Dyn,
    right: Dyn,
    shapes: &[Shape],
    active: &mut Vec<(usize, usize)>,
) -> R<bool> {
    crate::check_stack()?;
    if left.tag != right.tag
        && (left.tag < 3
            || right.tag < 3
            || left.tag == crate::FLOAT_TAG
            || right.tag == crate::FLOAT_TAG)
    {
        return Ok(false);
    }
    match left.tag {
        0 | 1 | 2 => return Ok(left.payload == right.payload),
        crate::FLOAT_TAG => {
            return Ok(bex_vm_types::float_order::eq(
                f64::from_bits(left.payload),
                f64::from_bits(right.payload),
            ));
        }
        3 => {
            return Ok(unsafe { Str::from_raw(left.payload as usize) }.as_str(cx)
                == unsafe { Str::from_raw(right.payload as usize) }.as_str(cx));
        }
        _ => {}
    }
    if left.tag == right.tag && left.payload == right.payload {
        return Ok(true);
    }
    if left.payload == 0 || right.payload == 0 {
        return Ok(false);
    }
    let pair = (left.payload as usize, right.payload as usize);
    if active.contains(&pair) {
        return Ok(true);
    }
    active.push(pair);
    let shape = &shapes[(left.tag - 3) as usize];
    let other_shape = &shapes[(right.tag - 3) as usize];
    let result = match (shape, other_shape) {
        (Shape::Bigint, Shape::Bigint) => unsafe {
            crate::native::bigint_owned(left.payload as usize)
                == crate::native::bigint_owned(right.payload as usize)
        },
        (Shape::Bytes, Shape::Bytes) => unsafe {
            crate::native::bytes_owned(left.payload as usize)
                == crate::native::bytes_owned(right.payload as usize)
        },
        (
            Shape::Class {
                declaration,
                fields,
            }
            | Shape::GenericClass {
                declaration,
                fields,
                ..
            },
            Shape::Class {
                declaration: other_declaration,
                fields: other_fields,
            }
            | Shape::GenericClass {
                declaration: other_declaration,
                fields: other_fields,
                ..
            },
        ) => {
            let mut same = left.tag == right.tag
                && declaration().tag == other_declaration().tag
                && fields.len() == other_fields.len();
            if !same {
                active.pop();
                return Ok(false);
            }
            for (field, other_field) in fields.iter().zip(*other_fields) {
                if !unsafe {
                    equal(
                        cx,
                        field_value(left, field),
                        field_value(right, other_field),
                        shapes,
                        active,
                    )?
                } {
                    same = false;
                    break;
                }
            }
            same
        }
        (Shape::List { .. }, Shape::List { .. }) => {
            let len = unsafe { list_len(left, shape) };
            let mut same = len == unsafe { list_len(right, other_shape) };
            if same {
                for i in 0..len {
                    if !unsafe {
                        equal(
                            cx,
                            list_value(left, shape, i),
                            list_value(right, other_shape, i),
                            shapes,
                            active,
                        )?
                    } {
                        same = false;
                        break;
                    }
                }
            }
            same
        }
        (Shape::Map { .. } | Shape::MapStringInt, Shape::Map { .. } | Shape::MapStringInt) => {
            let len_left = unsafe { map_len(left, shape) };
            let mut same = len_left == unsafe { map_len(right, other_shape) };
            if same {
                for i in 0..len_left {
                    let (key, value) = unsafe { map_entry(left, shape, i) };
                    let mut found = false;
                    for j in 0..len_left {
                        let (other_key, other_value) = unsafe { map_entry(right, other_shape, j) };
                        if unsafe { equal(cx, key, other_key, shapes, active)? } {
                            found = unsafe { equal(cx, value, other_value, shapes, active)? };
                            break;
                        }
                    }
                    if !found {
                        same = false;
                        break;
                    }
                }
            }
            same
        }
        (
            Shape::Enum { declaration, .. },
            Shape::Enum {
                declaration: other, ..
            },
        ) => declaration().tag == other().tag && left.payload == right.payload,
        _ => false,
    };
    active.pop();
    Ok(result)
}

pub(super) unsafe fn map_len(value: Dyn, shape: &Shape) -> usize {
    match shape {
        Shape::Map { len, .. } => unsafe { len(value.payload as usize) },
        Shape::MapStringInt => {
            unsafe { super::Map::<Str, i64>::from_raw(value.payload as usize) }.len()
        }
        _ => unreachable!(),
    }
}
pub(super) unsafe fn map_entry(value: Dyn, shape: &Shape, i: usize) -> (Dyn, Dyn) {
    match shape {
        Shape::Map { entry, .. } => unsafe { entry(value.payload as usize, i) },
        Shape::MapStringInt => {
            let (key, value) = unsafe { super::Map::<Str, i64>::from_raw(value.payload as usize) }
                .entry(i)
                .unwrap();
            (
                Dyn {
                    tag: 3,
                    payload: key.as_raw() as u64,
                },
                Dyn {
                    tag: 0,
                    payload: value as u64,
                },
            )
        }
        _ => unreachable!(),
    }
}

/// # Safety
/// `desc` describes baml.panics.UserPanic's single string field.
pub unsafe fn panic(cx: &mut Cx, desc: u32, message: &str) -> Box<crate::Error> {
    let value = (|| -> R<usize> {
        let text = cx.string(message)?;
        let value = unsafe { crate::try_alloc(desc, 16) }?;
        unsafe {
            baml_rt_gc::write_ptr(value, value.add(8).cast(), text.as_raw());
        }
        Ok(value as usize)
    })();
    match value {
        Err(error) => error,
        Ok(value) => Box::new(crate::Error {
            telemetry_raise: None,
            telemetry_at: None,
            value,
            class: "baml.panics.UserPanic",
            message: message.into(),
            frames: Vec::new(),
            display_value: Some(format!("baml.panics.UserPanic {{message: {message:?}}}")),
        }),
    }
}
