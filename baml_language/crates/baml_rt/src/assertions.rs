//! Structural comparison for assertions over erased values.
use crate::{Dyn, R, telemetry::capture::Shape};
use std::collections::BTreeSet;

/// The stdlib Type.to_string body rendered through a frozen spelling, with its
/// original function observation and Type-valued input capture.
/// # Safety
/// The string, site, observation word and schemas belong to this active program.
pub unsafe fn type_string<const OBSERVE: bool>(
    value: usize,
    site: usize,
    word: &std::sync::atomic::AtomicU8,
    schemas: &[(&str, &crate::telemetry::capture::Type)],
) -> R<usize> {
    if !OBSERVE {
        return Ok(value);
    }
    let mode = crate::telemetry::mode(word);
    if mode == 0 {
        return Ok(value);
    }
    if mode == 1 {
        let frame = unsafe { crate::telemetry::enter_observed(site) };
        let mut result = Ok(value);
        frame.finish(&mut result);
        return result;
    }
    let inputs = || unsafe { crate::telemetry::capture_frozen_arguments(&[value], schemas) };
    let output = |result: &R<usize>| match result {
        Ok(p) => unsafe {
            crate::telemetry::capture_value(Dyn {
                tag: 3,
                payload: *p as u64,
            })
        },
        Err(e) => unsafe { crate::telemetry::capture_error(e) },
    };
    unsafe {
        if mode > 3 {
            crate::telemetry::evidence::observe_captured(site, mode & 3, inputs, output, || {
                Ok(value)
            })
        } else {
            crate::telemetry::observe_captured(site, mode, inputs, output, || Ok(value))
        }
    }
}

/// # Safety
/// The operands and shape table belong to the active heap.
pub unsafe fn equal(a: Dyn, b: Dyn, shapes: &[Shape]) -> R<bool> {
    let roots = unsafe { &*crate::ROOTS.get() };
    let root = |v: Dyn| unsafe {
        roots.root(
            if v.tag >= 3
                && v.tag != 4294967294
                && !matches!(shapes[(v.tag - 3) as usize], Shape::Enum { .. })
            {
                v.payload as usize
            } else {
                0
            },
        )
    };
    let _a = root(a);
    let _b = root(b);
    let mut pending = vec![(a, b)];
    let mut visited = BTreeSet::new();
    while let Some((a, b)) = pending.pop() {
        let normalize = |v: Dyn| {
            if v.payload == 0
                && v.tag >= 3
                && v.tag != 4294967294
                && !matches!(shapes[(v.tag - 3) as usize], Shape::Enum { .. })
            {
                Dyn { tag: 2, payload: 0 }
            } else {
                v
            }
        };
        let (a, b) = (normalize(a), normalize(b));
        if a.tag != b.tag {
            let shape = |v: Dyn| {
                if v.tag >= 3 && v.tag != 4294967294 {
                    shapes.get((v.tag - 3) as usize)
                } else {
                    None
                }
            };
            if matches!(
                (shape(a), shape(b)),
                (Some(Shape::List { .. }), Some(Shape::List { .. }))
            ) {
                let left = unsafe { crate::format::children(shapes, a) };
                let right = unsafe { crate::format::children(shapes, b) };
                if left.len() != right.len() {
                    return Ok(false);
                }
                pending.extend(left.into_iter().zip(right));
                continue;
            }
            if !matches!(
                (shape(a), shape(b)),
                (
                    Some(Shape::MapStringInt | Shape::MapStringPtr { .. }),
                    Some(Shape::MapStringInt | Shape::MapStringPtr { .. })
                )
            ) {
                return Ok(false);
            }
        }
        if a.payload == b.payload {
            continue;
        }
        match a.tag {
            0 | 1 => return Ok(false),
            2 => continue,
            4294967294 => {
                if !bex_vm_types::float_order::eq(
                    f64::from_bits(a.payload),
                    f64::from_bits(b.payload),
                ) {
                    return Ok(false);
                }
                continue;
            }
            _ => {}
        }
        let key = if a.payload < b.payload {
            (a.payload, b.payload)
        } else {
            (b.payload, a.payload)
        };
        if !visited.insert(key) {
            continue;
        }
        match &shapes[(a.tag - 3) as usize] {
            Shape::String => {
                if !crate::strings::equals_raw(
                    &unsafe { crate::strings::Cx::current() },
                    a.payload as usize,
                    b.payload as usize,
                )? {
                    return Ok(false);
                }
            }
            Shape::Enum { .. } => return Ok(false),
            Shape::Class { .. } | Shape::GenericClass { .. } | Shape::List { .. } => {
                let left = unsafe { crate::format::children(shapes, a) };
                let right = unsafe { crate::format::children(shapes, b) };
                if left.len() != right.len() {
                    return Ok(false);
                }
                pending.extend(left.into_iter().zip(right));
            }
            Shape::MapStringInt | Shape::MapStringPtr { .. } => {
                let left = unsafe { crate::format::children(shapes, a) };
                let right = unsafe { crate::format::children(shapes, b) };
                if left.len() != right.len() {
                    return Ok(false);
                }
                for pair in left.chunks_exact(2) {
                    let mut matched = None;
                    for candidate in right.chunks_exact(2) {
                        if crate::strings::equals_raw(
                            &unsafe { crate::strings::Cx::current() },
                            pair[0].payload as usize,
                            candidate[0].payload as usize,
                        )? {
                            matched = Some(candidate[1]);
                            break;
                        }
                    }
                    let Some(value) = matched else {
                        return Ok(false);
                    };
                    pending.push((pair[1], value));
                }
            }
            Shape::Uint8Array => {
                if unsafe {
                    crate::bytes::data(a.payload as usize) != crate::bytes::data(b.payload as usize)
                } {
                    return Ok(false);
                }
            }
            _ => {
                return Err(crate::Error::internal(
                    "unsupported assertion comparison shape".into(),
                ));
            }
        }
    }
    Ok(true)
}
