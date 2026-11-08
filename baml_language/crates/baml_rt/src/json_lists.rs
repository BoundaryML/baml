//! Recursive typed JSON schemas for lists and their class/union elements.
use crate::{
    Dyn, R,
    lists::{List, Ptr, Slot},
    strings::{Cx, Str},
};
use baml_rt_gc::{SlotKind, write_ptr};
use serde_json::Value;
use smallvec::SmallVec;
mod stream;

#[derive(Clone, Copy)]
pub enum Ty {
    Int,
    Bigint {
        desc: u32,
    },
    Json(&'static crate::json_tree::Config),
    Float,
    Bool,
    Null,
    String,
    Literal(&'static str),
    LiteralInt(i64),
    LiteralFloat(f64),
    LiteralBool(bool),
    List {
        desc: u32,
        element: &'static Ty,
        slot: SlotKind,
    },
    Class {
        desc: u32,
        bytes: usize,
        name: &'static str,
        fields: &'static [Field],
    },
    Union(&'static [Member]),
}
pub struct Field {
    pub name: &'static str,
    pub offset: usize,
    pub ty: &'static Ty,
    pub slot: SlotKind,
    pub skip: bool,
}
pub struct Member {
    pub name: &'static str,
    pub ty: &'static Ty,
}
#[derive(Debug)]
struct Report {
    message: String,
    path: String,
}
fn fail(message: impl Into<String>, path: &str) -> Report {
    Report {
        message: message.into(),
        path: path.into(),
    }
}
impl Ty {
    pub(crate) fn nullable(&self) -> bool {
        match self {
            Self::Null | Self::Json(_) => true,
            Self::Union(m) => m.iter().any(|m| m.ty.nullable()),
            _ => false,
        }
    }
    fn score(&self, value: &Value) -> usize {
        match self {
            Self::Class { fields, .. } => value
                .as_object()
                .map(|o| {
                    fields
                        .iter()
                        .filter(|f| !f.skip && o.contains_key(f.name))
                        .map(|f| f.name)
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                })
                .unwrap_or(0),
            Self::Int if value.as_i64().is_some() => 1,
            _ => 0,
        }
    }
    fn member<'a>(&'a self, value: &Value, path: &str) -> Result<&'a Ty, Report> {
        let Self::Union(members) = self else {
            self.check(value, path)?;
            return Ok(self);
        };
        if members.len() == 2 && members.iter().any(|m| matches!(m.ty, Self::Null)) {
            let member = members
                .iter()
                .find(|m| matches!(m.ty, Self::Null) == value.is_null())
                .unwrap();
            member.ty.check(value, path)?;
            return Ok(member.ty);
        }
        let mut best = None;
        let mut reports = Vec::new();
        for member in *members {
            if matches!(member.ty, Self::Null) && !value.is_null() {
                continue;
            }
            match member.ty.check(value, path) {
                Ok(()) => {
                    let score = member.ty.score(value);
                    if best.is_none_or(|(_, previous)| score > previous) {
                        best = Some((member.ty, score));
                    }
                }
                Err(report) => reports.push((member.name, report)),
            }
        }
        if let Some((ty, _)) = best {
            return Ok(ty);
        }
        if reports.len() == 1 {
            return Err(reports.pop().unwrap().1);
        }
        let messages = reports
            .into_iter()
            .map(|(name, report)| {
                let relative = report.path.strip_prefix(path).unwrap_or(&report.path);
                if relative.is_empty() {
                    format!("`{name}`: {}", report.message)
                } else {
                    format!("`{name}`: {} at `{relative}`", report.message)
                }
            })
            .collect::<Vec<_>>();
        Err(fail(
            format!("no union member matched: {}", messages.join("; ")),
            path,
        ))
    }

    fn check(&self, value: &Value, path: &str) -> Result<(), Report> {
        let message = match self {
            Self::Json(_) => None,
            Self::Int => {
                if !value.is_number() {
                    Some("expected integer")
                } else if value.as_i64().and_then(baml_type::Int63::new).is_none() {
                    Some("expected integer in the BAML int range [-2^62, 2^62 - 1]")
                } else {
                    None
                }
            }
            Self::Bigint { .. } => {
                let text = if let Some(s) = value.as_str() {
                    Some(s.to_owned())
                } else if value.is_number() {
                    Some(value.to_string())
                } else {
                    None
                };
                if text
                    .as_deref()
                    .and_then(bex_vm_types::json_text::parse_decimal_bigint)
                    .is_some()
                {
                    None
                } else {
                    Some("expected integral JSON number or numeric string")
                }
            }
            Self::Float => {
                let numeric = value.as_f64();
                if numeric.is_some_and(|f| f.is_finite())
                    || (numeric.is_some()
                        && matches!(value.to_string().as_str(), "NaN" | "inf" | "-inf"))
                {
                    None
                } else {
                    Some("expected number")
                }
            }
            Self::Bool => {
                if value.is_boolean() {
                    None
                } else {
                    Some("expected boolean")
                }
            }
            Self::Null => {
                if value.is_null() {
                    None
                } else {
                    Some("expected null")
                }
            }
            Self::String => {
                if value.is_string() {
                    None
                } else {
                    Some("expected string")
                }
            }
            Self::Literal(text) => {
                if value.as_str() == Some(*text) {
                    None
                } else {
                    Some("literal mismatch")
                }
            }
            Self::LiteralInt(n) => {
                if value.as_i64() == Some(*n) {
                    None
                } else if value.is_number() {
                    Some("literal int mismatch")
                } else {
                    Some("literal mismatch")
                }
            }
            Self::LiteralFloat(n) => {
                if value
                    .as_f64()
                    .is_some_and(|v| (v - *n).abs() < f64::EPSILON)
                {
                    None
                } else if value.is_number() {
                    Some("literal float mismatch")
                } else {
                    Some("literal mismatch")
                }
            }
            Self::LiteralBool(b) => {
                if value.as_bool() == Some(*b) {
                    None
                } else {
                    Some("literal mismatch")
                }
            }
            Self::List { element, .. } => {
                let Some(values) = value.as_array() else {
                    return Err(fail("expected array", path));
                };
                for (i, v) in values.iter().enumerate() {
                    element.check(v, &format!("{path}[{i}]"))?;
                }
                None
            }
            Self::Class { name, fields, .. } => {
                let Some(object) = value.as_object() else {
                    return Err(fail(
                        format!("expected JSON object for class `{name}`"),
                        path,
                    ));
                };
                for field in *fields {
                    if field.skip {
                        continue;
                    }
                    match object.get(field.name) {
                        Some(v) => field.ty.check(v, &format!("{path}.{}", field.name))?,
                        None if field.ty.nullable() => {}
                        None => {
                            return Err(fail(
                                format!("missing required field `{}`", field.name),
                                path,
                            ));
                        }
                    }
                }
                None
            }
            Self::Union(_) => {
                self.member(value, path)?;
                None
            }
        };
        message.map_or(Ok(()), |message| Err(fail(message, path)))
    }
    fn empty(&self) -> Value {
        match self {
            Self::Json(_) => Value::Null,
            Self::Int | Self::Bigint { .. } => Value::from(0),
            Self::Float => Value::from(0.0),
            Self::Bool => Value::Bool(false),
            Self::Null => Value::Null,
            Self::String => Value::from(""),
            Self::Literal(s) => Value::from(*s),
            Self::LiteralInt(n) => Value::from(*n),
            Self::LiteralFloat(n) => Value::from(*n),
            Self::LiteralBool(b) => Value::from(*b),
            Self::List { .. } => Value::Array(vec![]),
            Self::Class { fields, .. } => Value::Object(
                fields
                    .iter()
                    .map(|f| (f.name.into(), f.ty.empty()))
                    .collect(),
            ),
            Self::Union(m) => {
                if self.nullable() {
                    Value::Null
                } else {
                    m[0].ty.empty()
                }
            }
        }
    }
    unsafe fn build(&self, cx: &mut Cx, value: &Value) -> R<Dyn> {
        Ok(match self {
            Self::Json(config) => unsafe { config.build(cx, value) }?,
            Self::Bigint { desc } => {
                let text = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                let pointer = unsafe { crate::bigint::from_decimal(cx, *desc, &text) }?;
                Dyn {
                    tag: *desc as u64 + 3,
                    payload: pointer.as_raw() as u64,
                }
            }
            Self::Int | Self::LiteralInt(_) => Dyn {
                tag: 0,
                payload: value.as_i64().unwrap() as u64,
            },
            Self::Float | Self::LiteralFloat(_) => Dyn {
                tag: (u32::MAX - 1) as u64,
                payload: value.as_f64().unwrap().to_bits(),
            },
            Self::Bool | Self::LiteralBool(_) => Dyn {
                tag: 1,
                payload: value.as_bool().unwrap() as u64,
            },
            Self::Null => Dyn { tag: 2, payload: 0 },
            Self::String | Self::Literal(_) => Dyn {
                tag: 3,
                payload: cx.string(value.as_str().unwrap())?.as_raw() as u64,
            },
            Self::Union(_) => unsafe {
                self.member(value, "")
                    .expect("validated union")
                    .build(cx, value)
            }?,
            Self::Class {
                desc,
                bytes,
                fields,
                ..
            } => {
                let ptr = unsafe { crate::try_alloc(*desc, *bytes) }?;
                let values = value.as_object().unwrap();
                for field in *fields {
                    let empty = if field.skip {
                        field.ty.empty()
                    } else {
                        Value::Null
                    };
                    let value = if field.skip {
                        &empty
                    } else {
                        values.get(field.name).unwrap_or(&Value::Null)
                    };
                    let decoded = unsafe { field.ty.build(cx, value) }?;
                    unsafe {
                        match field.slot {
                            SlotKind::Tagged => {
                                crate::write_dyn(ptr, ptr.add(field.offset).cast(), decoded)
                            }
                            SlotKind::Pointer => write_ptr(
                                ptr,
                                ptr.add(field.offset).cast(),
                                decoded.payload as usize,
                            ),
                            SlotKind::Scalar => {
                                ptr.add(field.offset).cast::<u64>().write(decoded.payload)
                            }
                        }
                    }
                }
                Dyn {
                    tag: *desc as u64 + 3,
                    payload: ptr as u64,
                }
            }
            Self::List {
                desc,
                element,
                slot,
            } => {
                let values = value.as_array().unwrap();
                unsafe fn fill<T: Slot>(
                    cx: &mut Cx,
                    desc: u32,
                    element: &Ty,
                    values: &[Value],
                    convert: impl Fn(Dyn) -> T,
                ) -> R<usize> {
                    let list = unsafe { List::<T>::with_capacity(cx, desc, values.len()) }?;
                    for (i, value) in values.iter().enumerate() {
                        let value = unsafe { element.build(cx, value) }?;
                        list.push(cx, convert(value))?;
                        if i & 255 == 255 {
                            cx.poll()?;
                        }
                    }
                    Ok(list.as_raw())
                }
                let pointer = unsafe {
                    match slot {
                        SlotKind::Tagged => fill::<Dyn>(cx, *desc, element, values, |v| v),
                        SlotKind::Pointer => fill::<Ptr>(cx, *desc, element, values, |v| {
                            Ptr::from_raw(v.payload as usize)
                        }),
                        SlotKind::Scalar => match element {
                            Ty::Float | Ty::LiteralFloat(_) => {
                                fill::<f64>(cx, *desc, element, values, |v| {
                                    f64::from_bits(v.payload)
                                })
                            }
                            Ty::Bool | Ty::LiteralBool(_) => {
                                fill::<bool>(cx, *desc, element, values, |v| v.payload != 0)
                            }
                            _ => fill::<i64>(cx, *desc, element, values, |v| v.payload as i64),
                        },
                    }
                }?;
                Dyn {
                    tag: *desc as u64 + 3,
                    payload: pointer as u64,
                }
            }
        })
    }
}
/// # Safety
/// Schema descriptors, offsets and slot kinds match the generated program.
pub unsafe fn decode(cx: &mut Cx, text: Str, ty: &'static Ty) -> R<Dyn> {
    // The reader borrows the non-moving string. Its interior pointer remains
    // on the scanned fiber stack through every managed allocation.
    let read_cx = unsafe { Cx::current() };
    let source = text.as_str(&read_cx);
    let mut syntax = bex_vm_types::json_text::Reader::new(source);
    if syntax.skip_value().is_err() || syntax.finish().is_err() {
        return Err(crate::json::syntax_error(cx, source));
    }
    if let Some(value) = unsafe { stream::decode(cx, source, ty) }? {
        return Ok(value);
    }
    let value = crate::json::parse_owned(cx, source)?;
    if let Err(report) = ty.check(&value, "") {
        return Err(crate::json::decode_error(cx, report.message, report.path));
    }
    unsafe { ty.build(cx, &value) }
}

pub enum Shape {
    Unsupported,
    Bigint,
    List {
        tag: u64,
        tagged: bool,
    },
    Class(&'static [EncodeField]),
    Map {
        len: unsafe fn(usize) -> usize,
        entry: unsafe fn(usize, usize) -> (Dyn, Dyn),
    },
}
pub struct EncodeField {
    pub name: &'static str,
    pub alias: &'static str,
    pub skip: bool,
    pub offset: usize,
    pub tag: u64,
    pub tagged: bool,
}
enum PathStep {
    Field(&'static str),
    Index(usize),
    Key(Str),
}
struct Encoder<'a> {
    shapes: &'static [Shape],
    cx: &'a mut Cx,
    aliased: bool,
    omit_nulls: bool,
    indent: String,
    out: String,
    ancestors: SmallVec<[usize; 16]>,
    path: SmallVec<[PathStep; 16]>,
}
impl Encoder<'_> {
    #[cold]
    fn error(&mut self, message: String, reason: &str) -> Box<crate::Error> {
        use std::fmt::Write;
        let mut path = String::new();
        for step in &self.path {
            match step {
                PathStep::Field(name) => write!(path, ".{name}").unwrap(),
                PathStep::Index(i) => write!(path, "[{i}]").unwrap(),
                PathStep::Key(key) => write!(path, "[{:?}]", key.as_str(self.cx)).unwrap(),
            }
        }
        crate::list_errors::serialization(self.cx, message, &path, reason)
    }
    fn quoted(&mut self, text: &str) {
        self.out.push('"');
        let mut start = 0;
        let bytes = text.as_bytes();
        while start < bytes.len() {
            let end = start + bex_vm_types::json_text::plain_run(&bytes[start..]);
            self.out.push_str(&text[start..end]);
            if end == bytes.len() {
                break;
            }
            match bytes[end] {
                b'"' => self.out.push_str("\\\""),
                b'\\' => self.out.push_str("\\\\"),
                b'\n' => self.out.push_str("\\n"),
                b'\r' => self.out.push_str("\\r"),
                b'\t' => self.out.push_str("\\t"),
                8 => self.out.push_str("\\b"),
                12 => self.out.push_str("\\f"),
                control => {
                    const HEX: &[u8; 16] = b"0123456789abcdef";
                    self.out.push_str("\\u00");
                    self.out.push(HEX[(control >> 4) as usize] as char);
                    self.out.push(HEX[(control & 15) as usize] as char);
                }
            }
            start = end + 1;
        }
        self.out.push('"');
    }
    fn separator(&mut self, first: bool, depth: usize) {
        if !first {
            self.out.push(',');
        }
        if !self.indent.is_empty() {
            self.newline(depth);
        }
    }
    fn newline(&mut self, depth: usize) {
        self.out.push('\n');
        for _ in 0..depth {
            self.out.push_str(&self.indent);
        }
    }
    unsafe fn value(&mut self, value: Dyn, depth: usize) -> R<()> {
        let value = if value.payload == 0 && value.tag >= 3 && value.tag != (u32::MAX - 1) as u64 {
            Dyn { tag: 2, payload: 0 }
        } else {
            value
        };
        match value.tag {
            0 => {
                self.out
                    .push_str(itoa::Buffer::new().format(value.payload as i64));
            }
            1 => self
                .out
                .push_str(if value.payload == 0 { "false" } else { "true" }),
            2 => self.out.push_str("null"),
            3 => {
                let text = unsafe { Str::from_raw(value.payload as usize) };
                let cx = unsafe { Cx::current() };
                self.quoted(text.as_str(&cx));
            }
            tag if tag == (u32::MAX - 1) as u64 => {
                let f = f64::from_bits(value.payload);
                if !f.is_finite() {
                    return Err(self.error(
                        format!("{f} has no JSON form: a JSON number is finite"),
                        "non_finite",
                    ));
                }
                serde_json::to_writer(unsafe { self.out.as_mut_vec() }, &f)
                    .expect("finite float JSON");
            }
            tag => {
                let pointer = value.payload as usize;
                if pointer == 0 {
                    self.out.push_str("null");
                    return Ok(());
                }
                if self.ancestors.contains(&pointer) {
                    return Err(self.error(
                        "value contains itself, so it has no JSON form".into(),
                        "cycle",
                    ));
                }
                self.ancestors.push(pointer);
                let shape = self
                    .shapes
                    .get((tag - 3) as usize)
                    .unwrap_or(&Shape::Unsupported);
                match shape {
                    Shape::Bigint => {
                        let text = unsafe { Str::from_raw(pointer) };
                        self.out.push_str(text.as_str(self.cx));
                    }
                    Shape::List { tag, tagged } => {
                        let ptr = pointer as *const usize;
                        let (buffer, len) = unsafe { (ptr.add(1).read(), ptr.add(2).read()) };
                        let data = (buffer + 16) as *const u8;
                        self.out.push('[');
                        for i in 0..len {
                            self.separator(i == 0, depth + 1);
                            let value = unsafe {
                                if *tagged {
                                    data.cast::<Dyn>().add(i).read()
                                } else {
                                    Dyn {
                                        tag: *tag,
                                        payload: data.cast::<u64>().add(i).read(),
                                    }
                                }
                            };
                            self.path.push(PathStep::Index(i));
                            unsafe { self.value(value, depth + 1) }?;
                            self.path.pop();
                            if i & 255 == 255 {
                                self.cx.poll()?;
                            }
                        }
                        if len > 0 && !self.indent.is_empty() {
                            self.newline(depth);
                        }
                        self.out.push(']');
                    }
                    Shape::Class(fields) => {
                        self.out.push('{');
                        let mut first = true;
                        for field in *fields {
                            if self.aliased && field.skip {
                                continue;
                            }
                            let slot = unsafe { (pointer as *const u8).add(field.offset) };
                            let value = unsafe {
                                if field.tagged {
                                    slot.cast::<Dyn>().read()
                                } else {
                                    Dyn {
                                        tag: field.tag,
                                        payload: slot.cast::<u64>().read(),
                                    }
                                }
                            };
                            if self.omit_nulls
                                && (value.tag == 2
                                    || (value.tag >= 3
                                        && value.tag != (u32::MAX - 1) as u64
                                        && value.payload == 0))
                            {
                                continue;
                            }
                            self.separator(first, depth + 1);
                            first = false;
                            let name = if self.aliased {
                                field.alias
                            } else {
                                field.name
                            };
                            self.quoted(name);
                            self.out.push(':');
                            if !self.indent.is_empty() {
                                self.out.push(' ');
                            }
                            self.path.push(PathStep::Field(name));
                            unsafe { self.value(value, depth + 1) }?;
                            self.path.pop();
                        }
                        if !first && !self.indent.is_empty() {
                            self.newline(depth);
                        }
                        self.out.push('}');
                    }
                    Shape::Map { len, entry } => {
                        let len = unsafe { len(pointer) };
                        self.out.push('{');
                        for i in 0..len {
                            let (key, value) = unsafe { entry(pointer, i) };
                            if key.tag != 3 || key.payload == 0 {
                                return Err(self.error(
                                    "JSON requires maps with string keys".into(),
                                    "map_key",
                                ));
                            }
                            let key = unsafe { Str::from_raw(key.payload as usize) };
                            self.separator(i == 0, depth + 1);
                            let read_cx = unsafe { Cx::current() };
                            self.quoted(key.as_str(&read_cx));
                            self.out.push(':');
                            if !self.indent.is_empty() {
                                self.out.push(' ');
                            }
                            self.path.push(PathStep::Key(key));
                            unsafe { self.value(value, depth + 1) }?;
                            // Keep the managed key on the native frame even if
                            // the path SmallVec has spilled into host storage.
                            std::hint::black_box(key);
                            self.path.pop();
                            if i & 255 == 255 {
                                self.cx.poll()?;
                            }
                        }
                        if len > 0 && !self.indent.is_empty() {
                            self.newline(depth);
                        }
                        self.out.push('}');
                    }
                    Shape::Unsupported => {
                        return Err(self.error("value has no JSON form".into(), "unserializable"));
                    }
                }
                self.ancestors.pop();
            }
        }
        Ok(())
    }
}
/// # Safety
/// Every dynamic tag and shape describes a live value in the current instance.
pub unsafe fn stringify(
    cx: &mut Cx,
    value: Dyn,
    shapes: &'static [Shape],
    aliased: bool,
    indent: Dyn,
    omit_nulls: bool,
) -> R<Str> {
    let indentation = if indent.tag == 0 {
        let n = indent.payload as i64;
        if !(0..=10).contains(&n) {
            return Err(crate::list_errors::serialization(
                cx,
                if n < 0 {
                    format!("indent must not be negative, got {n}")
                } else {
                    format!("indent must be at most 10 spaces, got {n}")
                },
                "",
                "indent",
            ));
        }
        " ".repeat(n as usize)
    } else {
        let text = unsafe { Str::from_raw(indent.payload as usize) }
            .as_str(cx)
            .to_owned();
        let width = text.chars().count();
        if width > 10 {
            return Err(crate::list_errors::serialization(
                cx,
                format!("indent must be at most 10 characters, got {width}"),
                "",
                "indent",
            ));
        }
        if !text.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
            return Err(crate::list_errors::serialization(
                cx,
                format!("indent must hold only spaces, tabs, and line breaks, got {text:?}"),
                "",
                "indent",
            ));
        }
        text
    };
    let mut encoder = Encoder {
        shapes,
        cx,
        aliased,
        omit_nulls,
        indent: indentation,
        out: String::with_capacity(64),
        ancestors: SmallVec::new(),
        path: SmallVec::new(),
    };
    unsafe { encoder.value(value, 0) }?;
    let result = encoder.out;
    encoder.cx.string(&result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*, lists};
    use baml_rt_gc::{Descriptor, Field as GcField, Kind, SlotKind};
    static DESCRIPTORS: &[Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
        INDEX_DESC,
        crate::json::PARSE_DESC,
        crate::json::DECODE_DESC,
        DIV_DESC,
        crate::list_errors::SERIALIZATION_DESC,
        crate::list_errors::INVALID_ARGUMENT_DESC,
        lists::descriptor::<i64>(15),
        lists::buffer_descriptor::<i64>(16),
        lists::descriptor::<Ptr>(17),
        lists::buffer_descriptor::<Ptr>(18),
        Descriptor {
            kind: Kind::Class,
            size: 16,
            ptr_slots: (8, 1),
            tagged_slots: (0, 0),
            elem: None,
            type_id: 19,
            fields: &[GcField {
                name: "values",
                offset: 8,
                slot: SlotKind::Pointer,
                type_id: 15,
            }],
            finalizer: None,
        },
        lists::descriptor::<Dyn>(20),
        lists::buffer_descriptor::<Dyn>(21),
    ];
    static INTS: Ty = Ty::List {
        desc: 12,
        element: &Ty::Int,
        slot: SlotKind::Scalar,
    };
    static ITEMS: Ty = Ty::List {
        desc: 14,
        element: &Ty::Class {
            desc: 16,
            bytes: 16,
            name: "Item",
            fields: &[Field {
                name: "values",
                offset: 8,
                ty: &INTS,
                slot: SlotKind::Pointer,
                skip: false,
            }],
        },
        slot: SlotKind::Pointer,
    };
    static SHAPES: &[Shape] = &[
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::Unsupported,
        Shape::List {
            tag: 0,
            tagged: false,
        },
        Shape::Unsupported,
        Shape::List {
            tag: 19,
            tagged: false,
        },
        Shape::Unsupported,
        Shape::Class(&[EncodeField {
            name: "values",
            alias: "items",
            skip: false,
            offset: 8,
            tag: 15,
            tagged: false,
        }]),
        Shape::List {
            tag: 0,
            tagged: true,
        },
        Shape::Unsupported,
    ];
    #[test]
    fn recursive_decode_encode_errors_and_cycles_survive_stress() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let input = cx.string("[{\"values\":[1,2]},{\"values\":[3]}]")?;
            let value = unsafe { decode(&mut cx, input, &ITEMS) }?;
            cx.poll()?;
            let compact = unsafe {
                stringify(
                    &mut cx,
                    value,
                    SHAPES,
                    false,
                    Dyn { tag: 0, payload: 0 },
                    false,
                )
            }?;
            assert_eq!(compact.as_str(&cx), "[{\"values\":[1,2]},{\"values\":[3]}]");
            let pretty = unsafe {
                stringify(
                    &mut cx,
                    value,
                    SHAPES,
                    true,
                    Dyn { tag: 0, payload: 2 },
                    false,
                )
            }?;
            assert_eq!(
                pretty.as_str(&cx),
                "[\n  {\n    \"items\": [\n      1,\n      2\n    ]\n  },\n  {\n    \"items\": [\n      3\n    ]\n  }\n]"
            );
            let bad = cx.string("[{\"values\":[1,\"bad\"]}]")?;
            let error = unsafe { decode(&mut cx, bad, &ITEMS) }.unwrap_err();
            assert_eq!(error.class, "baml.json.DecodeError");
            assert_eq!(error.message, "expected integer");
            assert!(error.display_value.unwrap().contains("[0].values[1]"));
            let cycle = unsafe { List::<Dyn>::new(&mut cx, 17) }?;
            let cyclic = Dyn {
                tag: 20,
                payload: cycle.as_raw() as u64,
            };
            cycle.push(&mut cx, cyclic)?;
            let error = unsafe {
                stringify(
                    &mut cx,
                    cyclic,
                    SHAPES,
                    false,
                    Dyn { tag: 0, payload: 0 },
                    false,
                )
            }
            .err()
            .expect("cycle must be rejected");
            assert_eq!(error.class, "baml.json.SerializationError");
            assert!(
                error
                    .display_value
                    .unwrap()
                    .contains("path: \"[0]\", reason: \"cycle\"")
            );
            let ints = unsafe { List::<i64>::filled(&mut cx, 12, 3, 7) }?;
            ints.splice(&mut cx, 1, 1, ints)?;
            assert_eq!(ints.len(&cx), 5);
            let error = ints.insert(&mut cx, 0, -6).unwrap_err();
            assert_eq!(error.class, "baml.errors.InvalidArgument");
            assert_eq!(
                error.message,
                "array.insert: idx (-6) is out of range for length 5"
            );
            assert_eq!(ints.len(&cx), 5);
            Ok(5)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true, true, true, true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 5);
    }
    #[test]
    fn quoted_strings_match_shared_json_escaping() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let mut encoder = Encoder {
                shapes: &[],
                cx: &mut cx,
                aliased: false,
                omit_nulls: false,
                indent: String::new(),
                out: String::new(),
                ancestors: SmallVec::new(),
                path: SmallVec::new(),
            };
            for code in 0..1024 {
                let ch = char::from_u32(code).unwrap();
                let text = format!(r#"prefix {ch} " \ suffix 🦀"#);
                encoder.out.clear();
                encoder.quoted(&text);
                assert_eq!(encoder.out, serde_json::to_string(&text).unwrap());
            }
            Ok(1)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true, true, true, true, true, true, true, true, true,
            ],
        );
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
    }
}

pub(crate) unsafe fn from_owned(cx: &mut Cx, ty: &Ty, value: &Value) -> R<Dyn> {
    if let Err(report) = ty.check(value, "") {
        return Err(crate::json::decode_error(cx, report.message, report.path));
    }
    unsafe { ty.build(cx, value) }
}
