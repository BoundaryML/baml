//! Typed decoding for flat class unions. Syntax is validated by the interpreter's
//! shared scanner before structural errors; object duplicates keep their last value.
use crate::{
    Dyn, Error, R,
    strings::{Cx, Str},
};
use bex_v2_heap::{Descriptor, Field, Kind, SlotKind, write_ptr};
use bex_vm_types::json_text::{Reader, Refusal, bigint_limit_message, refusal};
use serde_json::Value;
#[derive(Clone, Copy)]
pub enum Ty {
    String,
    Int,
    OptionalInt,
    Literal(&'static str),
    Literals(&'static [&'static str]),
}
pub struct SchemaField {
    pub name: &'static str,
    pub offset: usize,
    pub ty: Ty,
}
pub struct Schema {
    pub name: &'static str,
    pub desc: u32,
    pub bytes: usize,
    pub fields: &'static [SchemaField],
}
pub const PARSE_DESC: Descriptor = Descriptor {
    type_id: 10,
    ..crate::OVERFLOW_DESC
};
pub const DECODE_DESC: Descriptor = Descriptor {
    kind: Kind::Class,
    size: 24,
    ptr_slots: (8, 2),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 11,
    fields: &[
        Field {
            name: "message",
            offset: 8,
            slot: SlotKind::Pointer,
            type_id: 3,
        },
        Field {
            name: "path",
            offset: 16,
            slot: SlotKind::Pointer,
            type_id: 3,
        },
    ],
    finalizer: None,
};
#[derive(Debug)]
struct Report {
    message: String,
    path: String,
}
fn mismatch(message: impl Into<String>, path: &str) -> Report {
    Report {
        message: message.into(),
        path: path.into(),
    }
}
fn integer(value: &Value, path: &str) -> Result<i64, Report> {
    if !value.is_number() {
        return Err(mismatch("expected integer", path));
    }
    value
        .as_i64()
        .filter(|n| baml_type::Int63::new(*n).is_some())
        .ok_or_else(|| {
            mismatch(
                "expected integer in the BAML int range [-2^62, 2^62 - 1]",
                path,
            )
        })
}
fn check_field(value: &Value, ty: Ty, path: &str) -> Result<(), Report> {
    match ty {
        Ty::String => value
            .as_str()
            .map(|_| ())
            .ok_or_else(|| mismatch("expected string", path)),
        Ty::Int => integer(value, path).map(|_| ()),
        Ty::OptionalInt => {
            if value.is_null() {
                Ok(())
            } else {
                integer(value, path).map(|_| ())
            }
        }
        Ty::Literal(wanted) => {
            if value.as_str() == Some(wanted) {
                Ok(())
            } else {
                Err(mismatch("literal mismatch", path))
            }
        }
        Ty::Literals(wanted) => {
            if wanted.iter().any(|w| value.as_str() == Some(*w)) {
                Ok(())
            } else {
                let members = wanted
                    .iter()
                    .map(|w| format!("`{w:?}`: literal mismatch"))
                    .collect::<Vec<_>>()
                    .join("; ");
                Err(mismatch(
                    format!("no union member matched: {members}"),
                    path,
                ))
            }
        }
    }
}
fn check(value: &Value, schema: &Schema) -> Result<(), Report> {
    let object = value.as_object().ok_or_else(|| {
        mismatch(
            format!("expected JSON object for class `{}`", schema.name),
            "",
        )
    })?;
    for field in schema.fields {
        match object.get(field.name) {
            Some(value) => check_field(value, field.ty, &format!(".{}", field.name))?,
            None if matches!(field.ty, Ty::OptionalInt) => {}
            None => {
                return Err(mismatch(
                    format!("missing required field `{}`", field.name),
                    "",
                ));
            }
        }
    }
    Ok(())
}
fn error(cx: &mut Cx, message: String, path: Option<String>) -> Box<Error> {
    fn build(cx: &mut Cx, message: &str, path: Option<&str>) -> R<usize> {
        let msg = cx.string(message)?;
        let path = path.map(|p| cx.string(p)).transpose()?;
        let ptr = unsafe {
            crate::try_alloc(
                if path.is_some() { 8 } else { 7 },
                if path.is_some() { 24 } else { 16 },
            )
        }?;
        unsafe {
            write_ptr(ptr, ptr.add(8).cast(), msg.as_raw());
            if let Some(path) = path {
                write_ptr(ptr, ptr.add(16).cast(), path.as_raw());
            }
        }
        Ok(ptr as usize)
    }
    match build(cx, &message, path.as_deref()) {
        Err(e) => e,
        Ok(value) => Box::new(Error {
            value,
            class: if path.is_some() {
                "baml.json.DecodeError"
            } else {
                "baml.json.ParseError"
            },
            display_value: path
                .as_ref()
                .map(|p| format!("baml.json.DecodeError {{message: {message:?}, path: {p:?}}}")),
            message,
            frames: vec![],
        }),
    }
}
// Preserve the scanner's integral/float distinction, including integer -0.
// Out-of-i64 numbers only enter integer schema validation, where all are rejected.
fn owned(reader: &mut Reader<'_>) -> Result<Value, bex_vm_types::json_text::Stop> {
    use bex_vm_types::json_text::Stop;
    Ok(match reader.peek().ok_or(Stop)? {
        b'"' => Value::String(reader.string()?.to_owned()),
        b'n' => {
            reader.null()?;
            Value::Null
        }
        b't' | b'f' => Value::Bool(reader.bool()?),
        b'[' => {
            reader.open_array()?;
            let mut values = Vec::new();
            while reader.next_element(values.is_empty())? {
                values.push(owned(reader)?);
            }
            Value::Array(values)
        }
        b'{' => {
            reader.open_object()?;
            let mut values = serde_json::Map::new();
            while let Some(key) = reader.next_key(values.is_empty())? {
                let key = key.to_owned();
                values.insert(key, owned(reader)?);
            }
            Value::Object(values)
        }
        _ => {
            let number = reader.number()?;
            match number.as_i64() {
                Some(n) => Value::from(n),
                None => Value::from(number.as_f64().unwrap_or(1e100)),
            }
        }
    })
}
/// # Safety
/// Schemas describe actual class descriptors/offsets, including tagged optional
/// ints and literal unions. All handles obey the active Cx rooting contract.
pub unsafe fn decode(cx: &mut Cx, text: Str, schemas: &'static [Schema]) -> R<Dyn> {
    let source = text.as_str(cx).to_owned();
    let mut reader = Reader::new(&source);
    if reader.skip_value().is_err() || reader.finish().is_err() {
        let message = match refusal(&source) {
            Refusal::Syntax(m) | Refusal::OutOfRange(m) => m,
            Refusal::BigintLimit => bigint_limit_message(),
        };
        return Err(error(cx, message, None));
    }
    let value = owned(&mut Reader::new(&source)).expect("shared scanner accepted JSON");
    let mut order: Vec<usize> = (0..schemas.len()).collect();
    if let Some(object) = value.as_object() {
        order.sort_by_key(|i| {
            std::cmp::Reverse(
                schemas[*i]
                    .fields
                    .iter()
                    .filter(|f| object.contains_key(f.name))
                    .count(),
            )
        });
    }
    for i in order {
        let schema = &schemas[i];
        if check(&value, schema).is_err() {
            continue;
        }
        let ptr = unsafe { crate::try_alloc(schema.desc, schema.bytes) }?;
        // Object is zeroed and rooted in this frame across each string allocation.
        let object = value.as_object().unwrap();
        for field in schema.fields {
            let value = object.get(field.name).unwrap_or(&Value::Null);
            let slot = unsafe { ptr.add(field.offset) };
            match field.ty {
                Ty::Int => unsafe { slot.cast::<i64>().write(value.as_i64().unwrap()) },
                Ty::OptionalInt => unsafe {
                    crate::write_dyn(
                        ptr,
                        slot.cast(),
                        if value.is_null() {
                            Dyn { tag: 2, payload: 0 }
                        } else {
                            Dyn {
                                tag: 0,
                                payload: value.as_i64().unwrap() as u64,
                            }
                        },
                    )
                },
                Ty::String | Ty::Literal(_) => {
                    let string = cx.string(value.as_str().unwrap())?;
                    unsafe { write_ptr(ptr, slot.cast(), string.as_raw()) }
                }
                Ty::Literals(_) => {
                    let string = cx.string(value.as_str().unwrap())?;
                    unsafe {
                        crate::write_dyn(
                            ptr,
                            slot.cast(),
                            Dyn {
                                tag: 3,
                                payload: string.as_raw() as u64,
                            },
                        )
                    }
                }
            }
        }
        return Ok(Dyn {
            tag: u64::from(schema.desc) + 3,
            payload: ptr as u64,
        });
    }
    let reports: Vec<_> = schemas
        .iter()
        .map(|s| (s.name, check(&value, s).unwrap_err()))
        .collect();
    let (message, path) = if reports.len() == 1 {
        (reports[0].1.message.clone(), reports[0].1.path.clone())
    } else {
        (
            format!(
                "no union member matched: {}",
                reports
                    .iter()
                    .map(|(name, r)| if r.path.is_empty() {
                        format!("`{name}`: {}", r.message)
                    } else {
                        format!("`{name}`: {} at `{}`", r.message, r.path)
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            String::new(),
        )
    };
    Err(error(cx, message, Some(path)))
}
