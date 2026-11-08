//! Typed decoding for flat class unions. Syntax is validated by the interpreter's
//! shared scanner before structural errors; object duplicates keep their last value.
use crate::{
    Dyn, Error, R,
    strings::{Cx, Str},
};
use baml_rt_gc::{Descriptor, Field, Kind, SlotKind, write_ptr};
pub use bex_vm_types::json_text::Reader;
use bex_vm_types::json_text::{Refusal, bigint_limit_message, refusal};
use serde_json::Value;
#[derive(Clone, Copy)]
pub enum Ty {
    String,
    Int,
    OptionalInt,
    OptionalString,
    Literal(&'static str),
    Literals(&'static [&'static str]),
}
pub struct SchemaField {
    pub name: &'static str,
    pub offset: usize,
    pub ty: Ty,
    pub skip: bool,
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
        Ty::OptionalString if value.is_null() => Ok(()),
        Ty::String | Ty::OptionalString => value
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
        if field.skip {
            continue;
        }
        match object.get(field.name) {
            Some(value) => check_field(value, field.ty, &format!(".{}", field.name))?,
            None if matches!(field.ty, Ty::OptionalInt | Ty::OptionalString) => {}
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
pub(crate) fn error(cx: &mut Cx, message: String, path: Option<String>) -> Box<Error> {
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
            telemetry_raise: None,
            telemetry_at: None,
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
                None => Value::Number(number.text.parse().expect("validated JSON number")),
            }
        }
    })
}
pub(crate) fn parse_owned(cx: &mut Cx, source: &str) -> R<Value> {
    let mut reader = Reader::new(source);
    if reader.skip_value().is_err() || reader.finish().is_err() {
        return Err(syntax_error(cx, source));
    }
    Ok(owned(&mut Reader::new(source)).expect("shared scanner accepted JSON"))
}
pub(crate) fn decode_error(cx: &mut Cx, message: String, path: String) -> Box<Error> {
    error(cx, message, Some(path))
}
/// # Safety
/// Schemas describe actual class descriptors/offsets, including tagged optional
/// ints and literal unions. All handles obey the active Cx rooting contract.
pub unsafe fn decode(cx: &mut Cx, text: Str, schemas: &'static [Schema]) -> R<Dyn> {
    let source = text.try_as_str(cx)?.to_owned();
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
                    .filter(|f| !f.skip && object.contains_key(f.name))
                    .map(|f| f.name)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
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
            let empty = if field.skip {
                match field.ty {
                    Ty::String => Value::from(""),
                    Ty::Int => Value::from(0),
                    Ty::OptionalInt | Ty::OptionalString => Value::Null,
                    Ty::Literal(s) => Value::from(s),
                    Ty::Literals(values) => Value::from(values[0]),
                }
            } else {
                Value::Null
            };
            let value = if field.skip {
                &empty
            } else {
                object.get(field.name).unwrap_or(&Value::Null)
            };
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
                Ty::OptionalString if value.is_null() => {}
                Ty::String | Ty::OptionalString | Ty::Literal(_) => {
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

/// Stack-resident validated tokens for a generated concrete decoder. Unknown
/// fields are syntax checked and skipped; repeated fields overwrite their slot.
/// No JSON object or owned name per field is constructed. Escaped text uses the
/// shared scanner's reusable scratch; ordinary strings borrow the input.
#[derive(Clone, Copy)]
enum Token<'a> {
    String(&'a str),
    Escaped(bex_vm_types::json_text::Mark),
    Number(Option<i64>),
    Null,
    Other,
}
pub struct Stream<'a, const N: usize> {
    reader: Reader<'a>,
    fields: [Option<Token<'a>>; N],
    object: bool,
}
impl<'a, const N: usize> Stream<'a, N> {
    pub fn scan(
        source: &'a str,
        field: impl Fn(&str) -> Option<usize>,
    ) -> Result<Self, bex_vm_types::json_text::Stop> {
        let mut reader = Reader::new(source);
        let mut fields = [None; N];
        let object = reader.peek() == Some(b'{');
        if object {
            reader.open_object()?;
            let mut first = true;
            while let Some(key) = reader.next_key(first)? {
                first = false;
                let slot = field(key);
                if let Some(slot) = slot {
                    let next = reader.peek();
                    let mark = reader.mark();
                    fields[slot] = Some(match next {
                        Some(b'"') => match reader.borrowed_string()? {
                            Some(value) => Token::String(value),
                            None => Token::Escaped(mark),
                        },
                        Some(b'-' | b'0'..=b'9') => {
                            Token::Number(reader.validated_number()?.as_i64())
                        }
                        Some(b'n') => {
                            reader.null()?;
                            Token::Null
                        }
                        _ => {
                            reader.skip_value()?;
                            Token::Other
                        }
                    });
                } else {
                    reader.skip_value()?;
                }
            }
        } else {
            reader.skip_value()?;
        }
        reader.finish()?;
        Ok(Self {
            reader,
            fields,
            object,
        })
    }
    #[inline(always)]
    pub fn is_object(&self) -> bool {
        self.object
    }
    #[inline(always)]
    pub fn present(&self, field: usize) -> usize {
        usize::from(self.fields[field].is_some())
    }
    #[inline(always)]
    pub fn matches(&mut self, field: usize, ty: Ty) -> bool {
        if !self.object {
            return false;
        }
        let Some(token) = self.fields[field] else {
            return matches!(ty, Ty::OptionalInt | Ty::OptionalString);
        };
        match ty {
            Ty::String => matches!(token, Token::String(_) | Token::Escaped(_)),
            Ty::OptionalString => {
                matches!(token, Token::Null | Token::String(_) | Token::Escaped(_))
            }
            Ty::Int | Ty::OptionalInt => match token {
                Token::Null => matches!(ty, Ty::OptionalInt | Ty::OptionalString),
                Token::Number(Some(n)) => baml_type::Int63::new(n).is_some(),
                _ => false,
            },
            Ty::Literal(wanted) => self.text(token).is_some_and(|s| s == wanted),
            Ty::Literals(wanted) => self.text(token).is_some_and(|s| wanted.contains(&s)),
        }
    }
    #[inline(always)]
    fn text(&mut self, token: Token<'a>) -> Option<&str> {
        match token {
            Token::String(s) => Some(s),
            Token::Escaped(mark) => Some(self.reader.string_at(mark).expect("validated escape")),
            _ => None,
        }
    }
    /// # Safety
    /// The generated decoder validated this field with `matches`. `object` and
    /// its field offset describe the selected managed class. The source remains
    /// borrowed from its non-moving, conservatively rooted managed string.
    #[inline(always)]
    pub unsafe fn store(
        &mut self,
        cx: &mut Cx,
        object: *mut u8,
        offset: usize,
        field: usize,
        ty: Ty,
    ) -> R<()> {
        let token = self.fields[field];
        let slot = unsafe { object.add(offset) };
        match ty {
            Ty::OptionalString if matches!(token, None | Some(Token::Null)) => {}
            Ty::String | Ty::OptionalString | Ty::Literal(_) | Ty::Literals(_) => {
                let value = cx.string(
                    self.text(token.expect("validated field"))
                        .expect("validated string"),
                )?;
                unsafe {
                    if matches!(ty, Ty::Literals(_)) {
                        crate::write_dyn(
                            object,
                            slot.cast(),
                            Dyn {
                                tag: 3,
                                payload: value.as_raw() as u64,
                            },
                        );
                    } else {
                        write_ptr(object, slot.cast(), value.as_raw());
                    }
                }
            }
            Ty::Int => unsafe {
                slot.cast::<i64>().write(match token {
                    Some(Token::Number(Some(n))) => n,
                    _ => unreachable!("validated integer"),
                });
            },
            Ty::OptionalInt => {
                let value = if matches!(token, None | Some(Token::Null)) {
                    Dyn { tag: 2, payload: 0 }
                } else {
                    Dyn {
                        tag: 0,
                        payload: match token {
                            Some(Token::Number(Some(n))) => n,
                            _ => unreachable!("validated integer"),
                        } as u64,
                    }
                };
                unsafe {
                    crate::write_dyn(object, slot.cast(), value);
                }
            }
        }
        Ok(())
    }
    #[cold]
    fn report(&mut self, schema: &Schema, indices: &[usize]) -> Report {
        if !self.object {
            return mismatch(
                format!("expected JSON object for class `{}`", schema.name),
                "",
            );
        }
        for (field, &index) in schema.fields.iter().zip(indices) {
            if self.matches(index, field.ty) {
                continue;
            }
            let Some(token) = self.fields[index] else {
                return mismatch(format!("missing required field `{}`", field.name), "");
            };
            let message = match field.ty {
                Ty::String | Ty::OptionalString => "expected string".to_owned(),
                Ty::Int | Ty::OptionalInt => {
                    if matches!(token, Token::Number(_)) {
                        "expected integer in the BAML int range [-2^62, 2^62 - 1]".to_owned()
                    } else {
                        "expected integer".to_owned()
                    }
                }
                Ty::Literal(_) => "literal mismatch".to_owned(),
                Ty::Literals(wanted) => format!(
                    "no union member matched: {}",
                    wanted
                        .iter()
                        .map(|w| format!("`{w:?}`: literal mismatch"))
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
            };
            return mismatch(message, &format!(".{}", field.name));
        }
        unreachable!("report requested for a valid candidate")
    }
    #[cold]
    pub fn failure(&mut self, cx: &mut Cx, schemas: &[Schema], indices: &[&[usize]]) -> Box<Error> {
        let reports: Vec<_> = schemas
            .iter()
            .zip(indices)
            .map(|(s, i)| (s.name, self.report(s, i)))
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
        error(cx, message, Some(path))
    }
}
#[cold]
pub fn syntax_error(cx: &mut Cx, source: &str) -> Box<Error> {
    let message = match refusal(source) {
        Refusal::Syntax(m) | Refusal::OutOfRange(m) => m,
        Refusal::BigintLimit => bigint_limit_message(),
    };
    error(cx, message, None)
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    #[test]
    fn streaming_slots_replace_duplicates_and_validate_unknown_values() {
        let mut input = Stream::<2>::scan(
            r#"{"a":"wrong","extra":[{},null],"a":-0,"s":"a\u0062"}"#,
            |k| match k {
                "a" => Some(0),
                "s" => Some(1),
                _ => None,
            },
        )
        .unwrap();
        assert!(input.matches(0, Ty::Int));
        assert!(input.matches(1, Ty::Literal("ab")));
        assert_eq!(input.present(0), 1);
        assert!(Stream::<0>::scan(r#"{"unknown":[1,]}"#, |_| None).is_err());
        assert!(Stream::<0>::scan(r#"{} trailing"#, |_| None).is_err());
    }
    #[test]
    fn streaming_integer_semantics_do_not_coerce_float_string_or_bool() {
        for (token, valid) in [
            ("-4611686018427387904", true),
            ("4611686018427387903", true),
            ("4611686018427387904", false),
            ("1.0", false),
            ("true", false),
            ("\"1\"", false),
            ("null", false),
        ] {
            let text = format!("{{\"a\":{token}}}");
            let mut input = Stream::<1>::scan(&text, |_| Some(0)).unwrap();
            assert_eq!(input.matches(0, Ty::Int), valid, "{token}");
            assert_eq!(
                input.matches(0, Ty::OptionalInt),
                valid || token == "null",
                "{token}"
            );
        }
        let mut missing = Stream::<1>::scan("{}", |_| Some(0)).unwrap();
        assert!(missing.matches(0, Ty::OptionalInt));
        assert!(!missing.matches(0, Ty::Int));
    }
}

#[cfg(test)]
mod optional_string_tests {
    use super::*;
    #[test]
    fn nullable_strings_accept_absence_null_and_escaped_text() {
        for (source, expected) in [
            (r#"{}"#, true),
            (r#"{"s":null}"#, true),
            (r#"{"s":""}"#, true),
            (r#"{"s":"a\nb"}"#, true),
            (r#"{"s":42}"#, false),
            (r#"{"s":false}"#, false),
            (r#"{"s":[]}"#, false),
        ] {
            let mut input = Stream::<1>::scan(source, |key| (key == "s").then_some(0)).unwrap();
            assert_eq!(input.matches(0, Ty::OptionalString), expected, "{source}");
        }
    }
}
