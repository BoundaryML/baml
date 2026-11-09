//! `baml._equals_structural_default` over the closed-world capture shapes.
//!
//! Mirrors the interpreter's `EqualsDriver`: identical operands are equal by
//! reflexivity, a revisited pair is assumed equal so cycles terminate, arrays
//! and maps compare element-wise regardless of their static element types,
//! and class instances must share their concrete instantiation. The compiler
//! rejects programs whose compared classes or enums carry an explicit
//! `Equals` implementation, so this walk never calls user code and never
//! allocates in the managed heap: every visited object stays reachable from
//! the rooted operands, and the heap does not move.
use crate::{Dyn, FLOAT_TAG, R, strings::Cx, telemetry::capture::Shape};
use std::collections::HashSet;

const OMITTED: u64 = u64::MAX;

fn shape<'a>(shapes: &'a [Shape], value: Dyn) -> Option<&'a Shape> {
    if value.tag < 3 || value.tag == FLOAT_TAG || value.tag == OMITTED {
        return None;
    }
    shapes.get(usize::try_from(value.tag - 3).ok()?)
}

/// Pointer slots store null as a zero payload under their element tag.
fn normalize(shapes: &[Shape], value: Dyn) -> Dyn {
    if value.payload == 0
        && let Some(shape) = shape(shapes, value)
        && !matches!(shape, Shape::Enum { .. })
    {
        return Dyn { tag: 2, payload: 0 };
    }
    value
}

fn is_list(shape: Option<&Shape>) -> bool {
    matches!(shape, Some(Shape::List { .. }))
}
fn is_map(shape: Option<&Shape>) -> bool {
    matches!(
        shape,
        Some(Shape::Map { .. } | Shape::MapStringInt | Shape::MapStringPtr { .. })
    )
}

/// # Safety
/// Both operands and every value reachable from them are live values of the
/// active program, described by `shapes`.
pub unsafe fn structural(cx: &Cx, left: Dyn, right: Dyn, shapes: &[Shape]) -> R<bool> {
    let mut visited = HashSet::new();
    unsafe { walk(cx, left, right, shapes, &mut visited) }
}

unsafe fn walk(
    cx: &Cx,
    left: Dyn,
    right: Dyn,
    shapes: &[Shape],
    visited: &mut HashSet<(u64, u64)>,
) -> R<bool> {
    let mut stack = vec![(left, right)];
    while let Some((a, b)) = stack.pop() {
        let (a, b) = (normalize(shapes, a), normalize(shapes, b));
        if a.tag == b.tag && a.payload == b.payload {
            continue;
        }
        match (a.tag, b.tag) {
            (FLOAT_TAG, FLOAT_TAG) => {
                if !bex_vm_types::float_order::eq(
                    f64::from_bits(a.payload),
                    f64::from_bits(b.payload),
                ) {
                    return Ok(false);
                }
                continue;
            }
            (0..=2 | FLOAT_TAG | OMITTED, _) | (_, 0..=2 | FLOAT_TAG | OMITTED) => {
                return Ok(false);
            }
            _ => {}
        }
        let (sa, sb) = (shape(shapes, a), shape(shapes, b));
        let key = if a.payload < b.payload {
            (a.payload, b.payload)
        } else {
            (b.payload, a.payload)
        };
        if matches!(sa, Some(Shape::String)) && matches!(sb, Some(Shape::String)) {
            if !crate::strings::equals_raw(cx, a.payload as usize, b.payload as usize)? {
                return Ok(false);
            }
            continue;
        }
        let containers = (is_list(sa) && is_list(sb)) || (is_map(sa) && is_map(sb));
        if a.tag != b.tag && !containers {
            return Ok(false);
        }
        let Some(sa) = sa else {
            // Identity-only native slots (no shape) were settled above.
            return Ok(false);
        };
        match sa {
            Shape::String => {
                if !crate::strings::equals_raw(cx, a.payload as usize, b.payload as usize)? {
                    return Ok(false);
                }
            }
            Shape::Reflected(types) => unsafe {
                if types[crate::reflection::index(a.payload as usize)].identity
                    != types[crate::reflection::index(b.payload as usize)].identity
                {
                    return Ok(false);
                }
            },
            Shape::Bigint => unsafe {
                if crate::tasks::bigint_value(a.payload as usize)
                    != crate::tasks::bigint_value(b.payload as usize)
                {
                    return Ok(false);
                }
            },
            Shape::Uint8Array => unsafe {
                if crate::bytes::data(a.payload as usize) != crate::bytes::data(b.payload as usize)
                {
                    return Ok(false);
                }
            },
            Shape::Closure => unsafe {
                if !crate::closures::equal(a.payload as usize, b.payload as usize) {
                    return Ok(false);
                }
            },
            // Variants compare by identity; distinct payloads differ.
            Shape::Enum { .. } | Shape::Future | Shape::RustType => return Ok(false),
            Shape::Class { .. } | Shape::GenericClass { .. } | Shape::List { .. } => {
                if !visited.insert(key) {
                    continue;
                }
                let xs = unsafe { crate::format::children(shapes, a) };
                let ys = unsafe { crate::format::children(shapes, b) };
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                // Pop order follows the interpreter: last pushed pair first.
                stack.extend(xs.into_iter().zip(ys));
            }
            Shape::Map { .. } | Shape::MapStringInt | Shape::MapStringPtr { .. } => {
                if !visited.insert(key) {
                    continue;
                }
                let xs = unsafe { crate::format::children(shapes, a) };
                let ys = unsafe { crate::format::children(shapes, b) };
                if xs.len() != ys.len() {
                    return Ok(false);
                }
                // Keys are unique scalars or strings within a map, so at most
                // one candidate key matches and its value decides the entry.
                for entry in xs.chunks_exact(2) {
                    let mut matched = None;
                    for candidate in ys.chunks_exact(2) {
                        let mut scratch = visited.clone();
                        if unsafe { walk(cx, entry[0], candidate[0], shapes, &mut scratch) }? {
                            matched = Some(candidate[1]);
                            break;
                        }
                    }
                    let Some(value) = matched else {
                        return Ok(false);
                    };
                    stack.push((entry[1], value));
                }
            }
            Shape::Unsupported => {
                return Err(crate::Error::internal(
                    "unsupported structural equality shape".into(),
                ));
            }
        }
    }
    Ok(true)
}
