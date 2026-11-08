//! Recursive typed JSON schemas for lists and their class/union elements.
use crate::{
    Dyn, R,
    lists::{List, Ptr, Slot},
    strings::{Cx, Str},
};
use baml_rt_gc::{SlotKind, write_ptr};
use serde_json::Value;

#[derive(Clone, Copy)]
pub enum Ty {
    InvalidMap,
    Int,
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
    Map {
        key: &'static crate::telemetry::capture::Type,
        value: &'static Ty,
        adapter: &'static crate::maps::json::Adapter,
    },
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
    fn nullable(&self) -> bool {
        match self {
            Self::Null => true,
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
            Self::Map { .. } => value.as_object().map_or(0, |m| m.len()),
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
            Self::InvalidMap => Some("JSON requires maps with string keys"),
            Self::Int => {
                if !value.is_number() {
                    Some("expected integer")
                } else if value.as_i64().and_then(baml_type::Int63::new).is_none() {
                    Some("expected integer in the BAML int range [-2^62, 2^62 - 1]")
                } else {
                    None
                }
            }
            Self::Float => {
                if value.as_f64().is_some() {
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
            Self::Map {
                key,
                value: element,
                ..
            } => {
                if !crate::maps::json::string_key_type(key) {
                    return Err(fail("JSON requires maps with string keys", path));
                }
                let Some(values) = value.as_object() else {
                    return Err(fail("expected JSON object", path));
                };
                if values
                    .keys()
                    .any(|name| !crate::maps::json::key_fits(key, name))
                {
                    return Err(fail("map key does not match declared key type", path));
                }

                for (key, value) in values {
                    element.check(value, &format!("{path}[{key:?}]"))?;
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
            Self::InvalidMap => Value::Object(Default::default()),
            Self::Int => Value::from(0),
            Self::Float => Value::from(0.0),
            Self::Bool => Value::Bool(false),
            Self::Null => Value::Null,
            Self::String => Value::from(""),
            Self::Literal(s) => Value::from(*s),
            Self::LiteralInt(n) => Value::from(*n),
            Self::LiteralFloat(n) => Value::from(*n),
            Self::LiteralBool(b) => Value::from(*b),
            Self::List { .. } => Value::Array(vec![]),
            Self::Map { .. } => Value::Object(Default::default()),
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
            Self::InvalidMap => unreachable!("invalid map schema cannot build a value"),
            Self::Int | Self::LiteralInt(_) => Dyn {
                tag: 0,
                payload: value.as_i64().unwrap() as u64,
            },
            Self::Float | Self::LiteralFloat(_) => Dyn {
                tag: crate::FLOAT_TAG,
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
            Self::Map {
                value: element,
                adapter,
                ..
            } => {
                let map = unsafe { (adapter.new)(cx) }?;
                for (key, value) in value.as_object().unwrap() {
                    let decoded = unsafe { element.build(cx, value) }?;
                    let key = cx.string(key)?;
                    unsafe { (adapter.insert)(cx, map, key, decoded) }?;
                }
                Dyn {
                    tag: adapter.tag,
                    payload: map as u64,
                }
            }
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
    let source = text.as_str(cx).to_owned();
    let value = crate::json::parse_owned(cx, &source)?;
    if let Err(report) = ty.check(&value, "") {
        return Err(crate::json::decode_error(cx, report.message, report.path));
    }
    unsafe { ty.build(cx, &value) }
}

pub enum Shape {
    Unsupported,
    Enum(&'static [EnumVariant]),
    Bigint,
    Bytes,
    List {
        tag: u64,
        tagged: bool,
    },
    Class(&'static [EncodeField]),
    Map {
        key: &'static crate::telemetry::capture::Type,
        len: unsafe fn(usize) -> usize,
        entry: unsafe fn(usize, usize) -> (Dyn, Dyn),
    },
    Function,
}
pub struct EnumVariant {
    pub name: &'static str,
    pub alias: &'static str,
    pub skip: bool,
}
pub struct EncodeField {
    pub name: &'static str,
    pub alias: &'static str,
    pub skip: bool,
    pub offset: usize,
    pub tag: u64,
    pub tagged: bool,
}
struct Encoder<'a> {
    shapes: &'static [Shape],
    cx: &'a mut Cx,
    aliased: bool,
    omit_nulls: bool,
    indent: String,
    out: String,
    ancestors: std::collections::BTreeSet<usize>,
}
impl Encoder<'_> {
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
    unsafe fn value(&mut self, value: Dyn, path: &str, depth: usize) -> R<()> {
        if value.tag >= 3 && value.tag != crate::FLOAT_TAG {
            if let Some(Shape::Enum(variants)) = self.shapes.get((value.tag - 3) as usize) {
                let variant = &variants[value.payload as usize];
                if self.aliased && variant.skip {
                    return Err(crate::list_errors::serialization(
                        self.cx,
                        format!(
                            "enum variant `{}` is `@skip`, so JSON has no name for it",
                            variant.name
                        ),
                        path,
                        "enum",
                    ));
                }
                self.out.push_str(
                    &serde_json::to_string(if self.aliased {
                        variant.alias
                    } else {
                        variant.name
                    })
                    .expect("enum JSON"),
                );
                return Ok(());
            }
        }
        let value = if value.payload == 0 && value.tag >= 3 && value.tag != crate::FLOAT_TAG {
            Dyn { tag: 2, payload: 0 }
        } else {
            value
        };
        match value.tag {
            0 => self.out.push_str(&(value.payload as i64).to_string()),
            1 => self
                .out
                .push_str(if value.payload == 0 { "false" } else { "true" }),
            2 => self.out.push_str("null"),
            3 => {
                let text = unsafe { Str::from_raw(value.payload as usize) };
                self.out
                    .push_str(&serde_json::to_string(text.as_str(self.cx)).expect("string JSON"));
            }
            tag if tag == crate::FLOAT_TAG => {
                let f = f64::from_bits(value.payload);
                if !f.is_finite() {
                    return Err(crate::list_errors::serialization(
                        self.cx,
                        format!("{f} has no JSON form: a JSON number is finite"),
                        path,
                        "non_finite",
                    ));
                }
                self.out
                    .push_str(&serde_json::to_string(&f).expect("finite float JSON"));
            }
            tag => {
                let pointer = value.payload as usize;
                if pointer == 0 {
                    self.out.push_str("null");
                    return Ok(());
                }
                if !self.ancestors.insert(pointer) {
                    return Err(crate::list_errors::serialization(
                        self.cx,
                        "value contains itself, so it has no JSON form".into(),
                        path,
                        "cycle",
                    ));
                }
                let shape = self
                    .shapes
                    .get((tag - 3) as usize)
                    .unwrap_or(&Shape::Unsupported);
                match shape {
                    Shape::Enum(_) => unreachable!(),
                    Shape::Bigint => self
                        .out
                        .push_str(&unsafe { crate::native::bigint_owned(pointer) }.to_string()),
                    Shape::Bytes => {
                        return Err(crate::list_errors::serialization(
                            self.cx,
                            "uint8array requires explicit encoding (use to_base64() or to_hex())"
                                .into(),
                            path,
                            "uint8array",
                        ));
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
                            unsafe { self.value(value, &format!("{path}[{i}]"), depth + 1) }?;
                            if i & 255 == 255 {
                                self.cx.poll()?;
                            }
                        }
                        if len > 0 && !self.indent.is_empty() {
                            self.newline(depth);
                        }
                        self.out.push(']');
                    }
                    Shape::Map { key, len, entry } => {
                        if !crate::maps::json::string_key_type(key) {
                            return Err(crate::list_errors::serialization(
                                self.cx,
                                "JSON requires maps with string keys".into(),
                                path,
                                "map_key",
                            ));
                        }
                        self.out.push('{');
                        let count = unsafe { len(pointer) };
                        for i in 0..count {
                            let (key, value) = unsafe { entry(pointer, i) };
                            let name = unsafe { Str::from_raw(key.payload as usize) }
                                .as_str(self.cx)
                                .to_owned();
                            self.separator(i == 0, depth + 1);
                            self.out.push_str(&serde_json::to_string(&name).unwrap());
                            self.out.push(':');
                            if !self.indent.is_empty() {
                                self.out.push(' ');
                            }
                            unsafe { self.value(value, &format!("{path}[{name:?}]"), depth + 1) }?;
                        }
                        if count > 0 && !self.indent.is_empty() {
                            self.newline(depth);
                        }
                        self.out.push('}');
                    }
                    Shape::Function => {
                        return Err(crate::list_errors::serialization(
                            self.cx,
                            "a function has no JSON form".into(),
                            path,
                            "function",
                        ));
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
                                        && value.tag != crate::FLOAT_TAG
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
                            self.out
                                .push_str(&serde_json::to_string(name).expect("key JSON"));
                            self.out.push(':');
                            if !self.indent.is_empty() {
                                self.out.push(' ');
                            }
                            unsafe { self.value(value, &format!("{path}.{name}"), depth + 1) }?;
                        }
                        if !first && !self.indent.is_empty() {
                            self.newline(depth);
                        }
                        self.out.push('}');
                    }
                    Shape::Unsupported => {
                        return Err(crate::list_errors::serialization(
                            self.cx,
                            "value has no JSON form".into(),
                            path,
                            "unserializable",
                        ));
                    }
                }
                self.ancestors.remove(&pointer);
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
        out: String::new(),
        ancestors: std::collections::BTreeSet::new(),
    };
    unsafe { encoder.value(value, "", 0) }?;
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
                true, true, true, true, true, true, true, true, true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 5);
    }
}
