//! Map JSON decoding uses the VM's scanner. Repeated keys keep their original
//! position and their last value; syntax errors precede structural errors.
use super::{Map, Slot};
use crate::{
    R,
    strings::{Cx, Str},
};
pub use bex_vm_types::json_text::Reader;
use std::collections::HashMap;

/// Decode a map once JSON syntax has been validated. `decode` consumes one
/// value at the reader cursor, with the VM's jq-style path for diagnostics.
/// # Safety
/// desc is the descriptor quartet for Map<Str,V>; callbacks obey Cx's rooting
/// contract and leave no managed pointers in untraced host-owned storage.
pub unsafe fn decode<V: Slot>(
    cx: &mut Cx,
    desc: u32,
    text: Str,
    decode: impl FnMut(&mut Cx, &mut Reader<'_>, &str) -> R<V>,
) -> R<Map<Str, V>> {
    let read_cx = unsafe { Cx::current() };
    let source = text.as_str(&read_cx);
    if bex_vm_types::json_text::check(source).is_err() {
        return Err(crate::json::syntax_error(cx, source));
    }
    let mut reader = Reader::new(source);
    unsafe { decode_at(cx, desc, &mut reader, "", decode) }
}

/// Decode one object inside an already syntax-checked document.
/// # Safety
/// Same descriptor and rooting requirements as `decode`.
pub unsafe fn decode_at<V: Slot>(
    cx: &mut Cx,
    desc: u32,
    reader: &mut Reader<'_>,
    path: &str,
    mut decode: impl FnMut(&mut Cx, &mut Reader<'_>, &str) -> R<V>,
) -> R<Map<Str, V>> {
    if reader.open_object().is_err() {
        return Err(crate::json::error(
            cx,
            "expected JSON object".into(),
            Some(path.into()),
        ));
    }
    let mut positions = Vec::new();
    let mut index = HashMap::<String, usize>::new();
    let mut first = true;
    while let Some(key) = reader.next_key(first).expect("validated JSON") {
        first = false;
        let key = key.to_owned();
        let mark = reader.mark();
        if let Some(&slot) = index.get(&key) {
            positions[slot] = (key, mark);
        } else {
            index.insert(key.clone(), positions.len());
            positions.push((key, mark));
        }
        reader.skip_value().expect("validated JSON");
    }
    let end = reader.mark();
    let map = unsafe { Map::new(cx, desc) }?;
    for (key, mark) in positions {
        reader.reset(mark);
        let path = format!("{path}[{key:?}]");
        let value = decode(cx, reader, &path)?;
        let key = cx.string(&key)?;
        map.set(cx, key, value)?;
    }
    reader.reset(end);
    Ok(map)
}
pub fn integer(cx: &mut Cx, reader: &mut Reader<'_>, path: &str) -> R<i64> {
    if !matches!(reader.peek(), Some(b'-' | b'0'..=b'9')) {
        return Err(crate::json::error(
            cx,
            "expected integer".into(),
            Some(path.into()),
        ));
    }
    reader
        .validated_number()
        .ok()
        .and_then(|n| n.as_i64())
        .filter(|n| baml_type::Int63::new(*n).is_some())
        .ok_or_else(|| {
            crate::json::error(
                cx,
                "expected integer in the BAML int range [-2^62, 2^62 - 1]".into(),
                Some(path.into()),
            )
        })
}
pub fn string(cx: &mut Cx, reader: &mut Reader<'_>, path: &str) -> R<Str> {
    match reader.string() {
        Ok(value) => cx.string(value),
        Err(_) => Err(crate::json::error(
            cx,
            "expected string".into(),
            Some(path.into()),
        )),
    }
}
pub fn boolean(cx: &mut Cx, reader: &mut Reader<'_>, path: &str) -> R<bool> {
    reader
        .bool()
        .map_err(|_| crate::json::error(cx, "expected boolean".into(), Some(path.into())))
}

/// Serialize in insertion order. The callback appends each complete JSON value
/// directly, keeping value representation and nested-container traversal typed.
pub fn stringify<V: Slot>(
    cx: &mut Cx,
    map: Map<Str, V>,
    encode: impl FnMut(&mut Cx, V, &str, &mut String) -> R<()>,
) -> R<Str> {
    stringify_at(cx, map, "", encode)
}
/// Serialize a nested map while retaining its absolute error path.
pub fn stringify_at<V: Slot>(
    cx: &mut Cx,
    map: Map<Str, V>,
    prefix: &str,
    mut encode: impl FnMut(&mut Cx, V, &str, &mut String) -> R<()>,
) -> R<Str> {
    let entries = map.snapshot(cx)?;
    let mut out = String::from("{");
    for i in 0..entries.len() {
        let (key, value) = entries.entry(i).unwrap();
        if i != 0 {
            out.push(',');
        }
        let written = serde_json::to_string(key.as_str(cx)).expect("string serialization");
        let path = format!("{prefix}[{:?}]", key.as_str(cx));
        out.push_str(&written);
        out.push(':');
        encode(cx, value, &path, &mut out)?;
    }
    out.push('}');
    cx.string(&out)
}

pub fn write_string(out: &mut String, value: &str) {
    out.push_str(&serde_json::to_string(value).expect("JSON string"));
}

pub fn float(cx: &mut Cx, reader: &mut Reader<'_>, path: &str) -> R<f64> {
    if !matches!(reader.peek(), Some(b'-' | b'0'..=b'9')) {
        return Err(crate::json::error(
            cx,
            "expected number".into(),
            Some(path.into()),
        ));
    }
    reader
        .validated_number()
        .ok()
        .and_then(|n| n.as_f64())
        .ok_or_else(|| crate::json::error(cx, "expected number".into(), Some(path.into())))
}
pub fn null(cx: &mut Cx, reader: &mut Reader<'_>, path: &str) -> R<super::Null> {
    reader
        .null()
        .map(|_| super::Null)
        .map_err(|_| crate::json::error(cx, "expected null".into(), Some(path.into())))
}

/// Reformat verified compact JSON produced by the typed map encoder. Keeping
/// the original token bytes preserves insertion order and numeric spelling.
pub fn pretty(cx: &mut Cx, text: Str, indent: &str) -> R<Str> {
    if indent.is_empty() {
        return Ok(text);
    }
    let read = unsafe { Cx::current() };
    let source = text.as_str(&read).as_bytes();
    let mut output = Vec::with_capacity(source.len());
    let mut level = 0;
    let mut quoted = false;
    let mut escaped = false;
    fn newline(output: &mut Vec<u8>, level: usize, indent: &str) {
        output.push(b'\n');
        for _ in 0..level {
            output.extend_from_slice(indent.as_bytes());
        }
    }
    for (i, &byte) in source.iter().enumerate() {
        if quoted {
            output.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => {
                quoted = true;
                output.push(byte);
            }
            b'{' | b'[' => {
                output.push(byte);
                level += 1;
                if !matches!(source.get(i + 1), Some(b'}' | b']')) {
                    newline(&mut output, level, indent);
                }
            }
            b'}' | b']' => {
                level -= 1;
                if i != 0 && !matches!(source[i - 1], b'{' | b'[') {
                    newline(&mut output, level, indent);
                }
                output.push(byte);
            }
            b',' => {
                output.push(byte);
                newline(&mut output, level, indent);
            }
            b':' => output.extend_from_slice(b": "),
            _ => output.push(byte),
        }
    }
    cx.string(std::str::from_utf8(&output).expect("JSON and indent are UTF-8"))
}

/// Construct the ordinary language exception with its compiler-planned layout.
/// # Safety
/// `desc` describes SerializationError's three pointer fields in declaration order.
pub unsafe fn serialization_error(
    cx: &mut Cx,
    desc: u32,
    message: String,
    path: &str,
    reason: &str,
) -> Box<crate::Error> {
    let display = format!(
        "baml.json.SerializationError {{message: {message:?}, path: {path:?}, reason: {reason:?}}}"
    );
    let value = (|| -> R<usize> {
        let message = cx.string(&message)?;
        let path = cx.string(path)?;
        let reason = cx.string(reason)?;
        let value = unsafe { crate::try_alloc(desc, 32) }?;
        for (offset, text) in [(8, message), (16, path), (24, reason)] {
            unsafe { baml_rt_gc::write_ptr(value, value.add(offset).cast(), text.as_raw()) };
        }
        Ok(value as usize)
    })();
    match value {
        Err(error) => error,
        Ok(value) => Box::new(crate::Error {
            telemetry_raise: None,
            telemetry_at: None,
            value,
            class: "baml.json.SerializationError",
            message,
            frames: Vec::new(),
            display_value: Some(display),
        }),
    }
}
/// # Safety
/// The exception descriptor has the contract of `serialization_error`.
pub unsafe fn indent(cx: &mut Cx, value: crate::Dyn, desc: u32) -> R<String> {
    let message = match value.tag {
        0 => {
            let count = value.payload as i64;
            if (0..=10).contains(&count) {
                return Ok(" ".repeat(count as usize));
            }
            if count < 0 {
                format!("indent must not be negative, got {count}")
            } else {
                format!("indent must be at most 10 spaces, got {count}")
            }
        }
        3 => {
            let read = unsafe { Cx::current() };
            let text = unsafe { Str::from_raw(value.payload as usize) }.as_str(&read);
            let width = text.chars().count();
            if width > 10 {
                format!("indent must be at most 10 characters, got {width}")
            } else if text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) {
                return Ok(text.into());
            } else {
                format!("indent must hold only spaces, tabs, and line breaks, got {text:?}")
            }
        }
        _ => {
            return Err(crate::Error::internal(
                "invalid JSON indent representation".into(),
            ));
        }
    };
    Err(unsafe { serialization_error(cx, desc, message, "", "indent") })
}
/// # Safety
/// The exception descriptor has the contract of `serialization_error`.
pub unsafe fn write_float(
    cx: &mut Cx,
    value: f64,
    path: &str,
    desc: u32,
    out: &mut String,
) -> R<()> {
    if !value.is_finite() {
        return Err(unsafe {
            serialization_error(
                cx,
                desc,
                format!("{value} has no JSON form: a JSON number is finite"),
                path,
                "non_finite",
            )
        });
    }
    out.push_str(&serde_json::to_string(&value).expect("finite float"));
    Ok(())
}

/// # Safety
/// `value` has a scalar tag and all referenced strings belong to the active
/// instance; `desc` has the contract of `serialization_error`.
pub unsafe fn write_scalar(
    cx: &mut Cx,
    value: crate::Dyn,
    path: &str,
    desc: u32,
    out: &mut String,
) -> R<()> {
    match value.tag {
        0 => out.push_str(&(value.payload as i64).to_string()),
        1 => out.push_str(if value.payload != 0 { "true" } else { "false" }),
        2 => out.push_str("null"),
        3 => write_string(
            out,
            unsafe { Str::from_raw(value.payload as usize) }.as_str(cx),
        ),
        crate::FLOAT_TAG => {
            unsafe { write_float(cx, f64::from_bits(value.payload), path, desc, out) }?
        }
        _ => return Err(crate::Error::internal("non-scalar map JSON value".into())),
    }
    Ok(())
}
/// # Safety
/// `list` is a live growing integer list in the active instance.
pub unsafe fn write_int_list(list: usize, out: &mut String) -> R<()> {
    let cx = unsafe { Cx::current() };
    let list = unsafe { crate::lists::List::<i64>::from_raw(list) };
    let len = list.len(&cx);
    out.push('[');
    for i in 0..len {
        if i != 0 {
            out.push(',');
        }
        let value = list.get(&cx, i)?;
        out.push_str(&value.to_string());
    }
    out.push(']');
    Ok(())
}
pub fn write_string_list(cx: &Cx, list: crate::lists::List<Str>, out: &mut String) -> R<()> {
    out.push('[');
    for i in 0..list.len(cx) {
        if i != 0 {
            out.push(',');
        }
        write_string(out, list.get(cx, i)?.as_str(cx));
    }
    out.push(']');
    Ok(())
}
pub fn write_nested_string_list(
    cx: &Cx,
    list: crate::lists::List<crate::lists::List<Str>>,
    out: &mut String,
) -> R<()> {
    out.push('[');
    for i in 0..list.len(cx) {
        if i != 0 {
            out.push(',');
        }
        write_string_list(cx, list.get(cx, i)?, out)?;
    }
    out.push(']');
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_rt_gc::{Descriptor, Kind};
    static DESCRIPTORS: &[Descriptor] = &[
        crate::STRING_DESC,
        Descriptor {
            kind: Kind::Class,
            size: 32,
            ptr_slots: (8, 3),
            tagged_slots: (0, 0),
            elem: None,
            type_id: 4,
            fields: &[],
            finalizer: None,
        },
    ];
    #[test]
    fn non_finite_errors_trace_message_path_and_reason() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let error = unsafe {
                    write_float(
                        &mut cx,
                        number,
                        "[\"outer\"][\"bad\"]",
                        1,
                        &mut String::new(),
                    )
                }
                .unwrap_err();
                assert_eq!(error.class, "baml.json.SerializationError");
                assert_eq!(
                    error.message,
                    format!("{number} has no JSON form: a JSON number is finite")
                );
                let object = error.value;
                for _ in 0..32 {
                    cx.string("collect between construction and reading exception fields")?;
                }
                let path = unsafe { *((object + 16) as *const usize) };
                let reason = unsafe { *((object + 24) as *const usize) };
                assert_eq!(
                    unsafe { Str::from_raw(path) }.as_str(&cx),
                    "[\"outer\"][\"bad\"]"
                );
                assert_eq!(unsafe { Str::from_raw(reason) }.as_str(&cx), "non_finite");
            }
            Ok(0)
        }
        let mut instance = crate::Instance::new(DESCRIPTORS, &[false, false, false, true, true]);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 0);
    }
}

/// Typed map construction for recursive JSON schemas. Managed values never
/// live in the serde tree: each decoded value is stored in the traced map.
pub struct Adapter {
    pub tag: u64,
    pub new: unsafe fn(&mut Cx) -> R<usize>,
    pub insert: unsafe fn(&mut Cx, usize, Str, crate::Dyn) -> R<()>,
}
pub fn string_key_type(ty: &crate::telemetry::capture::Type) -> bool {
    use crate::telemetry::capture::Type;
    match ty {
        Type::String | Type::LiteralString(..) => true,
        Type::Union(m) => m.iter().all(string_key_type),
        _ => false,
    }
}

/// Whether a JSON property belongs to the declared string-key type.
pub fn key_fits(ty: &crate::telemetry::capture::Type, key: &str) -> bool {
    use crate::telemetry::capture::Type;
    match ty {
        Type::String => true,
        Type::LiteralString(value, _) => *value == key,
        Type::Union(m) => m.iter().any(|ty| key_fits(ty, key)),
        _ => false,
    }
}
