//! Structural text rendering with pre-order ToString override dispatch.
use crate::{
    Dyn, R,
    strings::{Cx, Str},
    telemetry::capture::Shape,
};
use baml_rt_gc::{Field, SlotKind};

unsafe fn field(pointer: usize, field: &Field) -> Dyn {
    let p = unsafe { (pointer as *const u8).add(field.offset) };
    match field.slot {
        SlotKind::Tagged => unsafe { p.cast::<Dyn>().read() },
        _ => Dyn {
            tag: u64::from(field.type_id),
            payload: unsafe { p.cast::<u64>().read() },
        },
    }
}
unsafe fn children(shapes: &[Shape], value: Dyn) -> Vec<Dyn> {
    if value.tag < 3 || value.tag == 4294967294 || value.payload == 0 {
        return vec![];
    }
    let pointer = value.payload as usize;
    match &shapes[(value.tag - 3) as usize] {
        Shape::Class { fields, .. } | Shape::GenericClass { fields, .. } => fields
            .iter()
            .map(|f| unsafe { field(pointer, f) })
            .collect(),
        Shape::List { growing, tag, .. } => {
            let p = pointer as *const usize;
            let (data, len) = if *growing {
                (unsafe { p.add(1).read() } as *const usize, unsafe {
                    p.add(2).read()
                })
            } else {
                (p, unsafe { p.add(1).read() })
            };
            (0..len)
                .map(|i| Dyn {
                    tag: *tag,
                    payload: unsafe { data.add(i + 2).read() } as u64,
                })
                .collect()
        }
        Shape::MapStringInt | Shape::MapStringPtr { .. } => {
            let p = pointer as *const usize;
            let data = (unsafe { p.add(1).read() } + 16) as *const u64;
            let len = unsafe { p.add(2).read() };
            (0..len)
                .flat_map(|i| {
                    [
                        Dyn {
                            tag: 3,
                            payload: unsafe { data.add(i * 2).read() },
                        },
                        Dyn {
                            tag: if let Shape::MapStringPtr { tag, .. } =
                                &shapes[(value.tag - 3) as usize]
                            {
                                *tag
                            } else {
                                0
                            },
                            payload: unsafe { data.add(i * 2 + 1).read() },
                        },
                    ]
                })
                .collect()
        }
        _ => vec![],
    }
}
unsafe fn collect(shapes: &[Shape], value: Dyn, tags: &[u64], pending: &mut Vec<Dyn>) {
    if value.tag < 4
        || value.tag == 4294967294
        || (value.payload == 0 && !matches!(shapes[(value.tag - 3) as usize], Shape::Enum { .. }))
    {
        return;
    }
    if tags.contains(&value.tag) {
        pending.push(value);
        return;
    }
    for child in unsafe { children(shapes, value) } {
        unsafe { collect(shapes, child, tags, pending) }
    }
}
/// # Safety
/// Values and metadata must describe the active heap. Overrides receive rooted
/// values and may allocate or mutate containers, exactly as ordinary BAML calls.
pub unsafe fn render(
    cx: &mut Cx,
    value: Dyn,
    shapes: &[Shape],
    tags: &[u64],
    mut invoke: impl FnMut(Dyn) -> R<Str>,
) -> R<Str> {
    let mut pending = vec![];
    unsafe { collect(shapes, value, tags, &mut pending) };
    // Native Vec storage is outside the conservative fiber stack. Keep every
    // pending receiver precisely rooted even if an earlier override removes it
    // from its original parent before its turn.
    let roots = crate::ROOTS.get();
    let leases: Vec<_> = pending
        .iter()
        .filter(|v| !matches!(shapes[(v.tag - 3) as usize], Shape::Enum { .. }))
        .map(|v| unsafe { (&*roots).root(v.payload as usize) })
        .collect();
    let mut results = Vec::with_capacity(pending.len());
    for &receiver in &pending {
        results.push(invoke(receiver)?.to_owned());
    }
    let mut out = String::new();
    unsafe { write(shapes, value, false, &pending, &results, &mut 0, &mut out) };
    drop(leases);
    cx.string(&out)
}
unsafe fn write(
    shapes: &[Shape],
    value: Dyn,
    nested: bool,
    pending: &[Dyn],
    results: &[String],
    cursor: &mut usize,
    out: &mut String,
) {
    match value.tag {
        0 => {
            out.push_str(&(value.payload as i64).to_string());
            return;
        }
        1 => {
            out.push_str(if value.payload == 0 { "false" } else { "true" });
            return;
        }
        2 => {
            out.push_str("null");
            return;
        }
        4294967294 => {
            out.push_str(&bex_vm_types::format_float(f64::from_bits(value.payload)));
            return;
        }
        _ => {}
    }
    if value.payload == 0 && !matches!(shapes[(value.tag - 3) as usize], Shape::Enum { .. }) {
        out.push_str("null");
        return;
    }
    if pending
        .get(*cursor)
        .is_some_and(|p| p.tag == value.tag && p.payload == value.payload)
    {
        out.push_str(&results[*cursor]);
        *cursor += 1;
        return;
    }
    match &shapes[(value.tag - 3) as usize] {
        Shape::String => {
            let text = unsafe { Str::from_raw(value.payload as usize) }.to_owned();
            out.push_str(&if nested { format!("{text:?}") } else { text });
        }
        Shape::Enum { variants, .. } => out.push_str(variants[value.payload as usize]),
        Shape::Class {
            declaration,
            fields,
        }
        | Shape::GenericClass {
            declaration,
            fields,
            ..
        } => {
            out.push_str(&declaration().name.item_name().to_string());
            if !fields.is_empty() {
                out.push_str(" { ");
            }
            for (i, f) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(f.name);
                out.push_str(": ");
                unsafe {
                    write(
                        shapes,
                        field(value.payload as usize, f),
                        true,
                        pending,
                        results,
                        cursor,
                        out,
                    )
                }
            }
            if !fields.is_empty() {
                out.push_str(" }");
            }
        }
        Shape::List { .. } => {
            out.push('[');
            for (i, v) in unsafe { children(shapes, value) }.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                unsafe { write(shapes, v, true, pending, results, cursor, out) }
            }
            out.push(']');
        }
        Shape::MapStringInt | Shape::MapStringPtr { .. } => {
            out.push('{');
            let values = unsafe { children(shapes, value) };
            for (i, pair) in values.chunks_exact(2).enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                unsafe { write(shapes, pair[0], true, pending, results, cursor, out) };
                out.push_str(": ");
                unsafe { write(shapes, pair[1], true, pending, results, cursor, out) }
            }
            out.push('}');
        }
        Shape::RustType => out.push_str("<rust_data>"),
        Shape::Uint8Array => {
            out.push('[');
            for (i, byte) in unsafe { crate::bytes::data(value.payload as usize) }
                .iter()
                .enumerate()
            {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&byte.to_string());
            }
            out.push(']');
        }
        _ => unreachable!("compiler checked structural formatting shape"),
    }
}
