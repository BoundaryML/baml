//! Thrown values retain their actual heap object and declaration-order fields.
use crate::{Dyn, Error, telemetry::capture::Shape};

/// Position of [`VALUE_DESC`] in every generated descriptor table.
pub const VALUE_DESC_ID: u32 = 17;
pub const VALUE_DESC: baml_rt_gc::Descriptor = baml_rt_gc::Descriptor {
    kind: baml_rt_gc::Kind::Class,
    size: 24,
    ptr_slots: (0, 0),
    tagged_slots: (8, 1),
    elem: None,
    type_id: VALUE_DESC_ID + 3,
    fields: &[],
    finalizer: None,
};

#[derive(Clone, Copy)]
pub struct ContextSchema {
    pub context: u32,
    pub trace: u32,
    pub frame: u32,
    pub frames: u32,
}

/// # Safety
/// The compiler supplies the standard library's pointer-first class layouts.
pub unsafe fn context(error: &Error, schema: ContextSchema) -> crate::R<usize> {
    let trace = error.trace.as_ref().unwrap_or(&error.frames);
    let frames = unsafe { crate::try_alloc(schema.frames, 16 + trace.len() * 8) }?;
    let roots = unsafe { &*crate::ROOTS.get() };
    let _frames = unsafe { roots.root(frames as usize) };
    unsafe {
        frames.add(8).cast::<usize>().write(trace.len());
    }
    let cx = unsafe { crate::strings::Cx::current() };
    for (i, frame) in trace.iter().rev().enumerate() {
        let file = cx.string(frame.file)?.as_raw();
        let _file = unsafe { roots.root(file) };
        let name = cx.string(frame.function)?.as_raw();
        let _name = unsafe { roots.root(name) };
        let object = unsafe { crate::try_alloc(schema.frame, 32) }?;
        unsafe {
            baml_rt_gc::write_ptr(object, object.add(8).cast(), file);
            baml_rt_gc::write_ptr(object, object.add(16).cast(), name);
            object.add(24).cast::<i64>().write(frame.line as i64);
            baml_rt_gc::write_ptr(frames, frames.add(16 + i * 8).cast(), object as usize);
        }
    }
    let stack = unsafe { crate::try_alloc(schema.trace, 16) }?;
    unsafe {
        baml_rt_gc::write_ptr(stack, stack.add(8).cast(), frames as usize);
    }
    let _stack = unsafe { roots.root(stack as usize) };
    let context = unsafe { crate::try_alloc(schema.context, 40) }?;
    unsafe {
        baml_rt_gc::write_ptr(context, context.add(8).cast(), stack as usize);
        baml_rt_gc::write_ptr(context, context.add(16).cast(), error.cause);
        crate::write_dyn(context, context.add(24).cast(), error.dynamic_value());
    }
    Ok(context as usize)
}

unsafe fn trace_text(stack: usize) -> String {
    let frames = unsafe { ((stack + 8) as *const usize).read() };
    let len = unsafe { ((frames + 8) as *const usize).read() };
    let frames = (0..len)
        .map(|i| unsafe {
            let frame = ((frames + 16 + i * 8) as *const usize).read();
            let file =
                crate::strings::Str::from_raw(((frame + 8) as *const usize).read()).to_owned();
            let name =
                crate::strings::Str::from_raw(((frame + 16) as *const usize).read()).to_owned();
            let line = ((frame + 24) as *const usize).read();
            (file, line, name)
        })
        .collect::<Vec<_>>();
    bex_vm_types::errors::format_traceback(
        frames.iter().map(|(f, l, n)| (f.as_str(), *l, n.as_str())),
    )
    .trim_end()
    .to_owned()
}

unsafe fn context_text(value: usize, shapes: &[Shape]) -> String {
    let mut out = String::new();
    let mut links = Vec::new();
    let mut current = value;
    while current != 0 {
        links.push(current);
        current = unsafe { ((current + 16) as *const usize).read() };
    }
    for link in links.into_iter().rev() {
        if !out.is_empty() {
            out.push_str("\n\nDuring handling of the above error, another error occurred:\n\n");
        }
        let trace = unsafe { ((link + 8) as *const usize).read() };
        let value = unsafe { ((link + 24) as *const Dyn).read() };
        out.push_str(&unsafe { trace_text(trace) });
        out.push('\n');
        out.push_str(&unsafe { render(value, shapes, false) });
    }
    out.trim_end().to_owned()
}

/// Snapshot test reporting before the fiber releases its managed roots.
/// # Safety
/// The error belongs to the active heap and its shape table.
pub unsafe fn test_failure(error: &Error, shapes: &[Shape]) -> String {
    if matches!(
        error.class,
        "baml.panics.AssertionFailed" | "baml.panics.UserPanic"
    ) {
        return error.message.clone();
    }
    let roots = unsafe { &*crate::ROOTS.get() };
    let _value = unsafe { roots.root(error.value) };
    let _context = unsafe { roots.root(error.context) };
    let _cause = unsafe { roots.root(error.cause) };
    if error.context != 0 {
        return unsafe { context_text(error.context, shapes) };
    }
    let frames = error.trace.as_ref().unwrap_or(&error.frames);
    let trace = bex_vm_types::errors::format_traceback(
        frames
            .iter()
            .rev()
            .filter(|f| !f.file.starts_with("<builtin>/"))
            .map(|f| (f.file, f.line, f.function)),
    );
    let mut out = if error.cause == 0 {
        String::new()
    } else {
        format!(
            "{}\n\nDuring handling of the above error, another error occurred:\n\n",
            unsafe { context_text(error.cause, shapes) }
        )
    };
    out.push_str(trace.trim_end());
    out.push('\n');
    out.push_str(&unsafe { render(error.dynamic_value(), shapes, false) });
    out.trim_end().to_owned()
}

/// # Safety
/// `value` is the indicated Context or StackTrace in the active heap.
pub unsafe fn to_string(value: usize, context: bool, shapes: &[Shape]) -> crate::R<usize> {
    let _root = unsafe { (&*crate::ROOTS.get()).root(value) };
    let out = if context {
        unsafe { context_text(value, shapes) }
    } else {
        unsafe { trace_text(value) }
    };
    unsafe { crate::strings::Cx::current() }
        .string(out.trim_end())
        .map(|s| s.as_raw())
}

/// # Safety
/// `value` and `shapes` describe the currently active heap.
pub unsafe fn thrown(value: Dyn, class: &'static str, shapes: &[Shape]) -> Box<Error> {
    let pointer = value.tag >= 3
        && value.tag != 4294967294
        && !matches!(shapes[(value.tag - 3) as usize], Shape::Enum { .. });
    let _root =
        unsafe { (&*crate::ROOTS.get()).root(if pointer { value.payload as usize } else { 0 }) };
    let mut message = String::new();
    if let Some(Shape::Class { fields, .. } | Shape::GenericClass { fields, .. }) = value
        .tag
        .checked_sub(3)
        .and_then(|i| shapes.get(i as usize))
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
    let diagnostic_value = Some(unsafe { render(value, shapes, false) });
    let is_class = matches!(
        value
            .tag
            .checked_sub(3)
            .and_then(|i| shapes.get(i as usize)),
        Some(Shape::Class { .. } | Shape::GenericClass { .. })
    );
    let object = if is_class {
        value.payload as usize
    } else {
        let p = match unsafe { crate::try_alloc(VALUE_DESC_ID, 24) } {
            Ok(p) => p,
            Err(e) => return e,
        };
        unsafe {
            crate::write_dyn(p, p.add(8).cast(), value);
        }
        p as usize
    };
    Box::new(Error {
        telemetry_caught: None,
        telemetry_history: Default::default(),
        telemetry_raise: None,
        telemetry_at: None,
        value: object,
        class,
        message,
        frames: vec![],
        thrown_value: (!is_class).then_some(value),
        exit_code: if class == "baml.panics.Exit" {
            // The builtin Exit has one scalar field, after its header.
            Some(unsafe { (value.payload as *const u8).add(8).cast::<i64>().read() })
        } else {
            None
        },
        context: 0,
        cause: 0,
        trace: None,
        diagnostic_value,
        display_value: Some(display),
    })
}

/// The VM uses compact debug fields for an escaping throw and a bounded,
/// qualified structural rendering for errors.Context.to_string.
unsafe fn render(value: Dyn, shapes: &[Shape], debug: bool) -> String {
    struct Renderer<'a> {
        shapes: &'a [Shape],
        debug: bool,
        remaining: usize,
        active: std::collections::BTreeSet<u64>,
    }
    impl Renderer<'_> {
        fn can_render(&self, depth: usize) -> bool {
            self.debug || self.remaining != 0 && depth <= 32
        }
        fn child_limit(&self) -> usize {
            if self.debug {
                usize::MAX
            } else {
                self.remaining
            }
        }
        unsafe fn value(&mut self, value: Dyn, nested: bool, depth: usize) -> String {
            if !self.debug {
                let available = self.remaining != 0;
                self.remaining = self.remaining.saturating_sub(1);
                if !available || depth > 32 {
                    return "…".into();
                }
            }
            let shapes = self.shapes;
            if self.debug && value.tag == 4294967294 {
                // BexExternalValue::render_readable keeps Rust's non-finite
                // spellings (inf, -inf, NaN) rather than format_float's.
                let f = f64::from_bits(value.payload);
                return if f.is_finite() {
                    bex_vm_types::format_float(f)
                } else {
                    f.to_string()
                };
            }
            if value.tag < 3 || value.tag == 4294967294 {
                return unsafe { crate::format::structural(value, shapes) };
            }
            let shape = &shapes[(value.tag - 3) as usize];
            if value.payload == 0 && !matches!(shape, Shape::Enum { .. }) {
                return "null".into();
            }
            if matches!(shape, Shape::String) {
                let text =
                    unsafe { crate::strings::Str::from_raw(value.payload as usize) }.to_owned();
                return if nested || self.debug {
                    format!("{text:?}")
                } else {
                    text
                };
            }
            if let Shape::Enum { variants, .. } = shape {
                return variants[value.payload as usize].into();
            }
            if !self.active.insert(value.payload) {
                return "…".into();
            }
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
                    let mut parts = Vec::new();
                    for field in *fields {
                        if !self.can_render(depth + 1) {
                            parts.push("…".into());
                            break;
                        }
                        let value = unsafe { crate::format::field(value.payload as usize, field) };
                        parts.push(format!("{}: {}", field.name, unsafe {
                            self.value(value, true, depth + 1)
                        }));
                    }
                    let fields = parts.join(", ");
                    if self.debug {
                        format!("{name} {{{fields}}}")
                    } else if parts.is_empty() {
                        name
                    } else {
                        format!("{name} {{ {fields} }}")
                    }
                }
                Shape::List { .. } => {
                    let (children, mut truncated) = unsafe {
                        crate::format::children_prefix(shapes, value, self.child_limit())
                    };
                    let mut parts = Vec::new();
                    for child in children {
                        if !self.can_render(depth + 1) {
                            truncated = true;
                            break;
                        }
                        parts.push(unsafe { self.value(child, true, depth + 1) });
                    }
                    if truncated {
                        parts.push("…".into());
                    }
                    format!("[{}]", parts.join(", "))
                }
                Shape::MapStringInt | Shape::MapStringPtr { .. } => {
                    let (children, mut truncated) = unsafe {
                        crate::format::children_prefix(shapes, value, self.child_limit())
                    };
                    let mut parts = Vec::new();
                    for pair in children.chunks_exact(2) {
                        if !self.can_render(depth + 1) {
                            truncated = true;
                            break;
                        }
                        let key = unsafe { self.value(pair[0], true, depth + 1) };
                        let value = unsafe { self.value(pair[1], true, depth + 1) };
                        parts.push(format!("{key}: {value}"));
                    }
                    if truncated {
                        parts.push("…".into());
                    }
                    format!("{{{}}}", parts.join(", "))
                }
                Shape::Uint8Array => {
                    let bytes = unsafe { crate::bytes::data(value.payload as usize) };
                    if self.debug {
                        format!("<bytes:{}>", bytes.len())
                    } else {
                        let limit = self.child_limit().min(bytes.len());
                        let mut parts =
                            bytes[..limit].iter().map(u8::to_string).collect::<Vec<_>>();
                        self.remaining = self.remaining.saturating_sub(limit);
                        if limit != bytes.len() {
                            parts.push("…".into());
                        }
                        format!("[{}]", parts.join(", "))
                    }
                }
                _ => unsafe { crate::format::structural(value, shapes) },
            };
            self.active.remove(&value.payload);
            rendered
        }
    }
    // Context diagnostics are bounded; the VM's escaping-value formatter renders
    // complete acyclic values and uses a compact byte-buffer summary.
    unsafe {
        Renderer {
            shapes,
            debug,
            remaining: 256,
            active: Default::default(),
        }
        .value(value, false, 0)
    }
}
