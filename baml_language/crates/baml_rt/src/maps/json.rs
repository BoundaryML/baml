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
            "expected object".into(),
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
        let path = format!("[{written}]");
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
