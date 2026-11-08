//! Thrown values retain their actual heap object and declaration-order fields.
use crate::{Dyn, Error, telemetry::capture::Shape};

/// # Safety
/// `value` and `shapes` describe the currently active heap.
pub unsafe fn thrown(value: Dyn, class: &'static str, shapes: &[Shape]) -> Box<Error> {
    let _root = unsafe { (&*crate::ROOTS.get()).root(value.payload as usize) };
    let mut message = String::new();
    if let Shape::Class { fields, .. } | Shape::GenericClass { fields, .. } =
        &shapes[(value.tag - 3) as usize]
    {
        if let Some(field) = fields
            .iter()
            .find(|f| f.name == "message" && f.type_id == 3)
        {
            let p = unsafe {
                (value.payload as *const u8)
                    .add(field.offset)
                    .cast::<usize>()
                    .read()
            };
            if p != 0 {
                message = unsafe { crate::strings::Str::from_raw(p) }.to_owned();
            }
        }
    }
    let display = unsafe { render(value, shapes, true) };
    Box::new(Error {
        telemetry_history: Default::default(),
        telemetry_raise: None,
        telemetry_at: None,
        value: value.payload as usize,
        class,
        message,
        frames: vec![],
        diagnostic_value: Some(unsafe { render(value, shapes, false) }),
        display_value: Some(display),
    })
}

/// The VM uses compact debug fields for an escaping throw and a bounded,
/// qualified structural rendering for errors.Context.to_string.
unsafe fn render(value: Dyn, shapes: &[Shape], debug: bool) -> String {
    unsafe fn visit(
        value: Dyn,
        shapes: &[Shape],
        debug: bool,
        nested: bool,
        depth: usize,
        budget: &mut usize,
        active: &mut std::collections::BTreeSet<u64>,
    ) -> String {
        if depth > 32 || *budget == 0 {
            return "…".into();
        }
        *budget -= 1;
        if value.tag < 3 || value.tag == 4294967294 {
            return unsafe { crate::format::structural(value, shapes) };
        }
        if value.payload == 0 && !matches!(shapes[(value.tag - 3) as usize], Shape::Enum { .. }) {
            return "null".into();
        }
        let shape = &shapes[(value.tag - 3) as usize];
        if matches!(shape, Shape::String) {
            let s = unsafe { crate::strings::Str::from_raw(value.payload as usize) }.to_owned();
            return if nested || debug { format!("{s:?}") } else { s };
        }
        if let Shape::Enum {
            declaration,
            variants,
        } = shape
        {
            return if debug {
                format!(
                    "{}.{}",
                    declaration().name,
                    variants[value.payload as usize]
                )
            } else {
                variants[value.payload as usize].into()
            };
        }
        if !active.insert(value.payload) {
            return "…".into();
        }
        let mut recur = |v| unsafe { visit(v, shapes, debug, true, depth + 1, budget, active) };
        let rendered = match shape {
            Shape::Class {
                declaration,
                fields,
            }
            | Shape::GenericClass {
                declaration,
                fields,
                ..
            } => {
                let name = declaration().name.to_string();
                let fields = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{}: {}",
                            f.name,
                            recur(unsafe { crate::format::field(value.payload as usize, f) })
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                if debug {
                    format!("{name} {{{fields}}}")
                } else if fields.is_empty() {
                    name
                } else {
                    format!("{name} {{ {fields} }}")
                }
            }
            Shape::List { .. } => format!(
                "[{}]",
                unsafe { crate::format::children(shapes, value) }
                    .into_iter()
                    .map(&mut recur)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Shape::MapStringInt | Shape::MapStringPtr { .. } => {
                let children = unsafe { crate::format::children(shapes, value) };
                let entries = children
                    .chunks_exact(2)
                    .map(|p| format!("{}: {}", recur(p[0]), recur(p[1])))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{entries}}}")
            }
            _ => unsafe { crate::format::structural(value, shapes) },
        };
        active.remove(&value.payload);
        rendered
    }
    unsafe {
        visit(
            value,
            shapes,
            debug,
            false,
            0,
            &mut 256,
            &mut std::collections::BTreeSet::new(),
        )
    }
}
