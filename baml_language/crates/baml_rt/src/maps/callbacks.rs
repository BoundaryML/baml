//! Hash/equality callbacks and managed structural snapshots for map keys.
use super::keys::Panic;
use crate::{Dyn, R, lists::List, strings::Cx, telemetry::capture::Shape};
use std::{cell::RefCell, collections::hash_map::DefaultHasher, hash::Hasher};

pub struct Hooks {
    pub shapes: &'static [Shape],
    pub hash: unsafe fn(Dyn, usize) -> R<bool>,
    pub equals: unsafe fn(Dyn, Dyn) -> R<Option<bool>>,
    pub panic: Panic,
    pub hasher_desc: u32,
    pub snapshot_desc: u32,
}
type State = std::rc::Rc<RefCell<DefaultHasher>>;

pub unsafe fn new_hasher(desc: u32) -> R<usize> {
    let state =
        unsafe { crate::native::new(&mut Cx::current(), RefCell::new(DefaultHasher::new()))? };
    let p = unsafe { crate::try_alloc(desc, 16)? };
    unsafe {
        baml_rt_gc::write_ptr(p, p.add(8).cast(), state);
    }
    Ok(p as usize)
}
unsafe fn state(hasher: usize) -> State {
    unsafe { crate::native::get((hasher as *const usize).add(1).read()) }
}
pub unsafe fn write(hasher: usize, bytes: &[u8]) {
    unsafe { state(hasher) }.borrow_mut().write(bytes);
}
pub unsafe fn finish(hasher: usize) -> u64 {
    unsafe { state(hasher) }.borrow().finish()
}
unsafe fn frame(hasher: usize, tag: u8, bytes: &[u8]) {
    unsafe {
        write(hasher, &[tag]);
        write(hasher, &(bytes.len() as u64).to_le_bytes());
        write(hasher, bytes);
    }
}

pub unsafe fn hash(cx: &mut Cx, value: Dyn, hooks: &Hooks) -> R<u64> {
    let hasher = unsafe { new_hasher(hooks.hasher_desc)? };
    unsafe {
        feed(cx, value, hasher, hooks, &mut Vec::new(), true)?;
    }
    Ok(unsafe { finish(hasher) })
}
pub unsafe fn hash_into(
    cx: &mut Cx,
    value: Dyn,
    hasher: usize,
    hooks: &Hooks,
    dispatch: bool,
) -> R<()> {
    unsafe { feed(cx, value, hasher, hooks, &mut Vec::new(), dispatch) }
}
fn shape(value: Dyn, hooks: &Hooks) -> &Shape {
    &hooks.shapes[(value.tag - 3) as usize]
}
unsafe fn snapshot(cx: &mut Cx, value: Dyn, hooks: &Hooks) -> R<List<Dyn>> {
    let s = shape(value, hooks);
    let (len, pairs) = match s {
        Shape::Class { fields, .. } | Shape::GenericClass { fields, .. } => (fields.len(), false),
        Shape::List { .. } => (unsafe { super::keys::list_len(value, s) }, false),
        Shape::Map { .. } | Shape::MapStringInt => {
            (unsafe { super::keys::map_len(value, s) }, true)
        }
        _ => unreachable!("structural snapshot"),
    };
    let result = unsafe {
        List::<Dyn>::with_capacity(cx, hooks.snapshot_desc, len * if pairs { 2 } else { 1 })?
    };
    for i in 0..len {
        match s {
            Shape::Class { fields, .. } | Shape::GenericClass { fields, .. } => {
                result.push(cx, unsafe { super::keys::field_value(value, &fields[i]) })?;
            }
            Shape::List { growing, tag, .. } => {
                let p = value.payload as *const usize;
                let p = if *growing {
                    unsafe { p.add(1).read() as *const usize }
                } else {
                    p
                };
                let child = if *tag == u64::MAX - 1 {
                    unsafe { p.add(2).cast::<Dyn>().add(i).read() }
                } else {
                    let payload = unsafe { p.add(2 + i).read() as u64 };
                    let tag = if *tag >= 3
                        && *tag != crate::FLOAT_TAG
                        && payload == 0
                        && !matches!(hooks.shapes[(*tag - 3) as usize], Shape::Enum { .. })
                    {
                        2
                    } else {
                        *tag
                    };
                    Dyn { tag, payload }
                };
                result.push(cx, child)?;
            }
            Shape::Map { .. } | Shape::MapStringInt => {
                let (key, value) = unsafe { super::keys::map_entry(value, s, i) };
                result.push(cx, key)?;
                result.push(cx, value)?;
            }
            _ => unreachable!(),
        }
    }
    Ok(result)
}
unsafe fn feed(
    cx: &mut Cx,
    value: Dyn,
    hasher: usize,
    hooks: &Hooks,
    active: &mut Vec<usize>,
    dispatch: bool,
) -> R<()> {
    crate::check_stack()?;
    if dispatch && unsafe { (hooks.hash)(value, hasher)? } {
        return Ok(());
    }
    unsafe {
        match value.tag {
            0 => {
                frame(hasher, 1, &value.payload.to_le_bytes());
                return Ok(());
            }
            1 => {
                write(hasher, &[2, u8::from(value.payload != 0)]);
                return Ok(());
            }
            2 => {
                write(hasher, &[0]);
                return Ok(());
            }
            crate::FLOAT_TAG => {
                let f = f64::from_bits(value.payload);
                let bits = if f.is_nan() {
                    f64::NAN.to_bits()
                } else if f == 0.0 {
                    0
                } else {
                    value.payload
                };
                frame(hasher, 3, &bits.to_le_bytes());
                return Ok(());
            }
            3 => {
                frame(
                    hasher,
                    5,
                    crate::strings::Str::from_raw(value.payload as usize)
                        .text(cx)
                        .as_bytes(),
                );
                return Ok(());
            }
            _ => {}
        }
        let s = shape(value, hooks);
        match s {
            Shape::Enum { declaration, .. } => {
                frame(hasher, 10, &declaration().tag.as_i64().to_le_bytes());
                write(hasher, &value.payload.to_le_bytes());
                return Ok(());
            }
            Shape::Bigint => {
                frame(
                    hasher,
                    4,
                    &crate::tasks::bigint_value(value.payload as usize).to_signed_bytes_le(),
                );
                return Ok(());
            }
            Shape::Uint8Array => {
                frame(hasher, 6, &crate::bytes::data(value.payload as usize));
                return Ok(());
            }
            _ => {}
        }
        if value.payload == 0 {
            write(hasher, &[0]);
            return Ok(());
        }
        if active.contains(&(value.payload as usize)) {
            return Err((hooks.panic)(cx, "Cannot hash a cyclic value"));
        }
        active.push(value.payload as usize);
        match s {
            Shape::Class {
                declaration,
                fields,
            }
            | Shape::GenericClass {
                declaration,
                fields,
                ..
            } => {
                let values = snapshot(cx, value, hooks)?;
                frame(hasher, 9, &declaration().tag.as_i64().to_le_bytes());
                write(hasher, &(fields.len() as u64).to_le_bytes());
                for i in 0..values.len(cx) {
                    feed(cx, values.get(cx, i as i64)?, hasher, hooks, active, true)?;
                }
            }
            Shape::List { .. } => {
                let values = snapshot(cx, value, hooks)?;
                frame(hasher, 7, &(values.len(cx) as u64).to_le_bytes());
                for i in 0..values.len(cx) {
                    feed(cx, values.get(cx, i as i64)?, hasher, hooks, active, true)?;
                }
            }
            Shape::Map { .. } | Shape::MapStringInt => {
                let values = snapshot(cx, value, hooks)?;
                let mut hashes = Vec::new();
                for i in (0..values.len(cx)).step_by(2) {
                    let item = new_hasher(hooks.hasher_desc)?;
                    feed(cx, values.get(cx, i as i64)?, item, hooks, active, true)?;
                    feed(
                        cx,
                        values.get(cx, (i + 1) as i64)?,
                        item,
                        hooks,
                        active,
                        true,
                    )?;
                    hashes.push(finish(item));
                }
                hashes.sort_unstable();
                write(hasher, &[8]);
                write(hasher, &((hashes.len() * 8) as u64).to_le_bytes());
                for h in hashes {
                    write(hasher, &h.to_le_bytes());
                }
            }
            _ => {
                return Err((hooks.panic)(
                    cx,
                    "Value does not support structural hashing",
                ));
            }
        }
        active.pop();
    }
    Ok(())
}
pub unsafe fn equals(cx: &mut Cx, a: Dyn, b: Dyn, hooks: &Hooks) -> R<bool> {
    unsafe { equal(cx, a, b, hooks, &mut Vec::new()) }
}
unsafe fn equal(
    cx: &mut Cx,
    a: Dyn,
    b: Dyn,
    hooks: &Hooks,
    active: &mut Vec<(u64, u64)>,
) -> R<bool> {
    crate::check_stack()?;
    if a.tag == b.tag && a.payload == b.payload {
        return Ok(true);
    }
    if a.tag < 4 || b.tag < 4 || a.tag == crate::FLOAT_TAG || b.tag == crate::FLOAT_TAG {
        return unsafe { super::keys::equals(cx, a, b, hooks.shapes) };
    }
    let s = shape(a, hooks);
    let t = shape(b, hooks);
    if a.payload == 0 || b.payload == 0 {
        return Ok(false);
    }
    let pair = if a.payload < b.payload {
        (a.payload, b.payload)
    } else {
        (b.payload, a.payload)
    };
    if active.contains(&pair) {
        return Ok(true);
    }
    active.push(pair);
    if let Some(result) = unsafe { (hooks.equals)(a, b)? } {
        return Ok(result);
    }
    if matches!(s, Shape::Enum { .. } | Shape::Bigint | Shape::Uint8Array)
        || matches!(t, Shape::Enum { .. } | Shape::Bigint | Shape::Uint8Array)
    {
        return unsafe { super::keys::equals(cx, a, b, hooks.shapes) };
    }
    let comparable = match (s, t) {
        (
            Shape::Class { declaration: d, .. } | Shape::GenericClass { declaration: d, .. },
            Shape::Class { declaration: e, .. } | Shape::GenericClass { declaration: e, .. },
        ) => a.tag == b.tag && d().tag == e().tag,
        (Shape::List { .. }, Shape::List { .. }) => true,
        (Shape::Map { .. } | Shape::MapStringInt, Shape::Map { .. } | Shape::MapStringInt) => true,
        _ => false,
    };
    let result = if !comparable {
        false
    } else {
        unsafe {
            let left = snapshot(cx, a, hooks)?;
            let right = snapshot(cx, b, hooks)?;
            let mut same = left.len(cx) == right.len(cx);
            if same && matches!(s, Shape::Map { .. } | Shape::MapStringInt) {
                for i in (0..left.len(cx)).step_by(2) {
                    let key = left.get(cx, i as i64)?;
                    let value = left.get(cx, (i + 1) as i64)?;
                    let mut found = false;
                    for j in (0..right.len(cx)).step_by(2) {
                        if equal(cx, key, right.get(cx, j as i64)?, hooks, active)? {
                            found =
                                equal(cx, value, right.get(cx, (j + 1) as i64)?, hooks, active)?;
                            break;
                        }
                    }
                    if !found {
                        same = false;
                        break;
                    }
                }
            } else if same {
                for i in 0..left.len(cx) {
                    if !equal(
                        cx,
                        left.get(cx, i as i64)?,
                        right.get(cx, i as i64)?,
                        hooks,
                        active,
                    )? {
                        same = false;
                        break;
                    }
                }
            }
            same
        }
    };
    Ok(result)
}

/// A synchronous collection keeps callback tests on the owner thread and roots.
pub unsafe fn collect() {
    let _ = crate::telemetry::sysop(|| {
        unsafe {
            (&mut *crate::HEAP.get()).collect(&crate::Roots(&*crate::ROOTS.get()));
        }
        Ok(())
    });
}
