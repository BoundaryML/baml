//! Managed recursive JSON values and heap-neutral selector traversal.
use crate::{
    Dyn, Error, R,
    json_lists::{Shape, Ty},
    lists::List,
    maps::Map,
    strings::{Cx, Str},
};
use bex_vm_types::json_text::{Reader, Stop};
use serde_json::Value;
use smallvec::SmallVec;
pub struct Config {
    pub list: u32,
    pub map: u32,
    pub bigint: u32,
    pub path_error: u32,
    pub shapes: &'static [Shape],
}
const NULL: Dyn = Dyn { tag: 2, payload: 0 };
impl Config {
    fn shape(&self, v: Dyn) -> Option<&Shape> {
        if v.tag < 3 || v.tag == crate::FLOAT_TAG || v.payload == 0 {
            return None;
        }
        self.shapes.get((v.tag - 3) as usize)
    }
    fn array_len(&self, cx: &Cx, v: Dyn) -> Option<usize> {
        matches!(self.shape(v), Some(Shape::List { .. }))
            .then(|| unsafe { List::<Dyn>::from_raw(v.payload as usize) }.len(cx) as usize)
    }
    fn element(&self, v: Dyn, index: usize) -> Dyn {
        unsafe {
            let Some(Shape::List { tag, tagged }) = self.shape(v) else {
                unreachable!()
            };
            let object = v.payload as *const usize;
            let data = (object.add(1).read() + 16) as *const u8;
            if *tagged {
                data.cast::<Dyn>().add(index).read()
            } else {
                Dyn {
                    tag: *tag,
                    payload: data.cast::<u64>().add(index).read(),
                }
            }
        }
    }
    fn object_len(&self, v: Dyn) -> Option<usize> {
        if let Some(Shape::Map { len, .. }) = self.shape(v) {
            Some(unsafe { len(v.payload as usize) })
        } else {
            None
        }
    }
    fn entry(&self, v: Dyn, index: usize) -> (Dyn, Dyn) {
        let Some(Shape::Map { entry, .. }) = self.shape(v) else {
            unreachable!()
        };
        unsafe { entry(v.payload as usize, index) }
    }
    fn get(&self, cx: &Cx, v: Dyn, key: &str) -> Option<Dyn> {
        if v.tag == self.map as u64 + 3 && v.payload != 0 {
            return unsafe { Map::<Str, Dyn>::from_raw(v.payload as usize) }.get_text(cx, key);
        }
        for i in 0..self.object_len(v)? {
            let (k, value) = self.entry(v, i);
            if k.tag == 3 && unsafe { Str::from_raw(k.payload as usize) }.as_str(cx) == key {
                return Some(value);
            }
        }
        None
    }
    /// Convert an already syntax-checked owned tree to managed layouts.
    pub(crate) unsafe fn build(&self, cx: &mut Cx, value: &Value) -> R<Dyn> {
        Ok(match value {
            Value::Null => NULL,
            Value::Bool(b) => Dyn {
                tag: 1,
                payload: *b as u64,
            },
            Value::String(s) => Dyn {
                tag: 3,
                payload: cx.string(s)?.as_raw() as u64,
            },
            Value::Number(n) => {
                if let Some(n) = n.as_i64().and_then(baml_type::Int63::new) {
                    Dyn {
                        tag: 0,
                        payload: n.get() as u64,
                    }
                } else {
                    let text = n.to_string();
                    if !text.contains(['.', 'e', 'E']) {
                        Dyn {
                            tag: self.bigint as u64 + 3,
                            payload: unsafe { crate::bigint::from_decimal(cx, self.bigint, &text) }?
                                .as_raw() as u64,
                        }
                    } else {
                        Dyn {
                            tag: crate::FLOAT_TAG,
                            payload: n.as_f64().unwrap().to_bits(),
                        }
                    }
                }
            }
            Value::Array(values) => {
                let list = unsafe { List::<Dyn>::with_capacity(cx, self.list, values.len()) }?;
                for value in values {
                    let value = unsafe { self.build(cx, value) }?;
                    list.push(cx, value)?;
                }
                Dyn {
                    tag: self.list as u64 + 3,
                    payload: list.as_raw() as u64,
                }
            }
            Value::Object(values) => {
                let map = unsafe { Map::<Str, Dyn>::new(cx, self.map) }?;
                for (key, value) in values {
                    let key = cx.string(key)?;
                    let value = unsafe { self.build(cx, value) }?;
                    map.set(cx, key, value)?;
                }
                Dyn {
                    tag: self.map as u64 + 3,
                    payload: map.as_raw() as u64,
                }
            }
        })
    }
    unsafe fn read(&self, cx: &mut Cx, r: &mut Reader<'_>) -> Result<Dyn, ReadError> {
        Ok(match r.peek().ok_or(Stop)? {
            b'"' => Dyn {
                tag: 3,
                payload: cx.string(r.string()?)?.as_raw() as u64,
            },
            b'n' => {
                r.null()?;
                NULL
            }
            b't' | b'f' => Dyn {
                tag: 1,
                payload: r.bool()? as u64,
            },
            b'[' => {
                r.open_array()?;
                let list = unsafe { List::<Dyn>::new(cx, self.list) }?;
                let mut count = 0;
                while r.next_element(count == 0)? {
                    let value = unsafe { self.read(cx, r) }?;
                    list.push(cx, value)?;
                    count += 1;
                    if count & 255 == 0 {
                        cx.poll()?;
                    }
                }
                Dyn {
                    tag: self.list as u64 + 3,
                    payload: list.as_raw() as u64,
                }
            }
            b'{' => {
                r.open_object()?;
                let map = unsafe { Map::<Str, Dyn>::new(cx, self.map) }?;
                let mut first = true;
                while let Some(key) = r.next_key(first)? {
                    first = false;
                    let key = cx.string(key)?;
                    let value = unsafe { self.read(cx, r) }?;
                    map.set(cx, key, value)?;
                }
                Dyn {
                    tag: self.map as u64 + 3,
                    payload: map.as_raw() as u64,
                }
            }
            _ => {
                let number = r.validated_number()?;
                if number.integral {
                    if let Some(n) = number.as_i64().and_then(baml_type::Int63::new) {
                        Dyn {
                            tag: 0,
                            payload: n.get() as u64,
                        }
                    } else {
                        Dyn {
                            tag: self.bigint as u64 + 3,
                            payload: unsafe {
                                crate::bigint::from_decimal(cx, self.bigint, number.text)
                            }?
                            .as_raw() as u64,
                        }
                    }
                } else {
                    Dyn {
                        tag: crate::FLOAT_TAG,
                        payload: number.as_f64().ok_or(Stop)?.to_bits(),
                    }
                }
            }
        })
    }
    fn path_error(&self, cx: &mut Cx, message: String, selector: &str) -> Box<Error> {
        let pointer = match unsafe { crate::try_alloc(self.path_error, 24) } {
            Ok(p) => p,
            Err(e) => return e,
        };
        for (offset, text) in [(8, message.as_str()), (16, selector)] {
            let text = match cx.string(text) {
                Ok(s) => s,
                Err(e) => return e,
            };
            unsafe {
                baml_rt_gc::write_ptr(pointer, pointer.add(offset).cast(), text.as_raw());
            }
        }
        Box::new(Error {
            telemetry_at: None,
            telemetry_raise: None,
            value: pointer as usize,
            class: "baml.json.PathError",
            display_value: Some(format!(
                "baml.json.PathError {{message: {message:?}, selector: {selector:?}}}"
            )),
            message,
            frames: vec![],
        })
    }
}
enum ReadError {
    Syntax,
    Runtime(Box<Error>),
}
impl From<Stop> for ReadError {
    fn from(_: Stop) -> Self {
        Self::Syntax
    }
}
impl From<Box<Error>> for ReadError {
    fn from(e: Box<Error>) -> Self {
        Self::Runtime(e)
    }
}
/// # Safety
/// All Config descriptors match their growing containers in the active instance.
pub unsafe fn parse(cx: &mut Cx, config: &Config, text: Str) -> R<Dyn> {
    let read_cx = unsafe { Cx::current() };
    let source = text.as_str(&read_cx);
    let mut syntax = Reader::new(source);
    if syntax.skip_value().is_err() || syntax.finish().is_err() {
        return Err(crate::json::syntax_error(cx, source));
    }
    match unsafe { config.read(cx, &mut Reader::new(source)) } {
        Ok(v) => Ok(v),
        Err(ReadError::Runtime(e)) => Err(e),
        Err(ReadError::Syntax) => unreachable!("validated JSON"),
    }
}
/// The whole selector is retained in every PathError; offsets count characters.
/// # Safety
/// Every dynamic value uses the layouts in Config.
pub unsafe fn path(cx: &mut Cx, config: &Config, mut value: Dyn, selector: Str) -> R<Dyn> {
    let read_cx = unsafe { Cx::current() };
    let selector = selector.as_str(&read_cx);
    let bytes = selector.as_bytes();
    if !matches!(bytes.first(), Some(b'.' | b'[')) {
        return Err(config.path_error(
            cx,
            format!("selector must start with '.' or '[' (jq style), got '{selector}'"),
            selector,
        ));
    }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            i += 1;
            let start = i;
            while i < bytes.len() && !matches!(bytes[i], b'.' | b'[') {
                i += 1;
            }
            if i > start {
                value = field_required(cx, config, value, &selector[start..i], selector)?;
            }
            continue;
        }
        if bytes[i] != b'[' {
            return Err(config.path_error(
                cx,
                format!(
                    "expected '.' or '[' at offset {}",
                    selector[..i].chars().count()
                ),
                selector,
            ));
        }
        i += 1;
        let Some(&first) = bytes.get(i) else {
            return Err(config.path_error(cx, "unterminated '[' segment".into(), selector));
        };
        if matches!(first, b'\'' | b'"') {
            i += 1;
            let mut key = String::new();
            let mut closed = false;
            while let Some(ch) = selector[i..].chars().next() {
                i += ch.len_utf8();
                if ch == '\\' {
                    let Some(ch) = selector[i..].chars().next() else {
                        return Err(config.path_error(
                            cx,
                            "unterminated escape in quoted bracket key".into(),
                            selector,
                        ));
                    };
                    key.push(ch);
                    i += ch.len_utf8();
                    continue;
                }
                if ch as u32 == first as u32 {
                    closed = true;
                    break;
                }
                key.push(ch);
            }
            if !closed || bytes.get(i) != Some(&b']') {
                return Err(config.path_error(
                    cx,
                    "unterminated quoted bracket key".into(),
                    selector,
                ));
            }
            i += 1;
            value = field_required(cx, config, value, &key, selector)?;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b']' {
            i += 1;
        }
        if i == bytes.len() {
            return Err(config.path_error(cx, "unterminated '[' segment".into(), selector));
        }
        let token = &selector[start..i];
        i += 1;
        let Some(index) = token
            .parse::<i64>()
            .ok()
            .and_then(baml_type::Int63::new)
            .map(|n| n.get())
        else {
            return Err(config.path_error(cx, format!("bad bracket token '[{token}]'"), selector));
        };
        let Some(len) = config.array_len(cx, value) else {
            return Err(config.path_error(cx, format!("index [{index}] on a non-array"), selector));
        };
        if index < 0 || index as usize >= len {
            return Err(config.path_error(
                cx,
                format!("index [{index}] out of bounds (length {len})"),
                selector,
            ));
        }
        value = config.element(value, index as usize);
    }
    Ok(value)
}
fn field_required(cx: &mut Cx, config: &Config, value: Dyn, key: &str, selector: &str) -> R<Dyn> {
    if config.object_len(value).is_none() {
        return Err(config.path_error(
            cx,
            format!("field access '.{key}' on a non-object"),
            selector,
        ));
    }
    config
        .get(cx, value, key)
        .ok_or_else(|| config.path_error(cx, format!("missing field '{key}'"), selector))
}
/// # Safety
/// The dynamic value uses Config's layouts and `key` is a live string.
pub unsafe fn field(cx: &Cx, config: &Config, value: Dyn, key: Str) -> Dyn {
    config.get(cx, value, key.as_str(cx)).unwrap_or(NULL)
}

struct Visiting {
    value: Dyn,
    next: usize,
    len: usize,
    map: bool,
}
impl Config {
    fn validate(&self, cx: &mut Cx, value: Dyn) -> R<()> {
        let mut open = SmallVec::<[Visiting; 16]>::new();
        let mut next = Some(value);
        loop {
            if let Some(value) = next.take() {
                let container = if let Some(len) = self.array_len(cx, value) {
                    Some((len, false))
                } else {
                    self.object_len(value).map(|len| (len, true))
                };
                if let Some((len, map)) = container {
                    if open.iter().any(|f| f.value.payload == value.payload) {
                        let path = self.validation_path(cx, &open);
                        return Err(crate::json::decode_error(
                            cx,
                            "cyclic json value: the document contains itself".into(),
                            path,
                        ));
                    }
                    if map && (0..len).any(|i| self.entry(value, i).0.tag != 3) {
                        let path = self.validation_path(cx, &open);
                        return Err(crate::json::decode_error(
                            cx,
                            "JSON requires maps with string keys".into(),
                            path,
                        ));
                    }
                    open.push(Visiting {
                        value,
                        next: 0,
                        len,
                        map,
                    });
                }
            }
            let Some(frame) = open.last_mut() else {
                return Ok(());
            };
            if frame.next == frame.len {
                open.pop();
                continue;
            }
            let index = frame.next;
            frame.next += 1;
            next = Some(if frame.map {
                self.entry(frame.value, index).1
            } else {
                self.element(frame.value, index)
            });
        }
    }
    #[cold]
    fn validation_path(&self, cx: &Cx, open: &[Visiting]) -> String {
        use std::fmt::Write;
        let mut text = String::new();
        for frame in open {
            if frame.next == 0 {
                continue;
            }
            if frame.map {
                let key = self.entry(frame.value, frame.next - 1).0;
                if key.tag == 3 {
                    write!(
                        text,
                        "[{:?}]",
                        unsafe { Str::from_raw(key.payload as usize) }.as_str(cx)
                    )
                    .unwrap();
                }
            } else {
                write!(text, "[{}]", frame.next - 1).unwrap();
            }
        }
        text
    }
    fn owned(&self, cx: &Cx, value: Dyn) -> Value {
        match value.tag {
            0 => Value::from(value.payload as i64),
            1 => Value::Bool(value.payload != 0),
            2 => Value::Null,
            3 => Value::String(
                unsafe { Str::from_raw(value.payload as usize) }
                    .as_str(cx)
                    .into(),
            ),
            crate::FLOAT_TAG => {
                let number = f64::from_bits(value.payload);
                if number.is_finite() {
                    Value::from(number)
                } else {
                    Value::Number(serde_json::Number::from_string_unchecked(
                        number.to_string(),
                    ))
                }
            }
            _ => {
                if value.tag == self.bigint as u64 + 3 {
                    Value::Number(
                        unsafe { Str::from_raw(value.payload as usize) }
                            .as_str(cx)
                            .parse()
                            .expect("normalized bigint"),
                    )
                } else if let Some(len) = self.array_len(cx, value) {
                    Value::Array(
                        (0..len)
                            .map(|i| self.owned(cx, self.element(value, i)))
                            .collect(),
                    )
                } else if let Some(len) = self.object_len(value) {
                    Value::Object(
                        (0..len)
                            .map(|i| {
                                let (k, v) = self.entry(value, i);
                                (
                                    unsafe { Str::from_raw(k.payload as usize) }
                                        .as_str(cx)
                                        .into(),
                                    self.owned(cx, v),
                                )
                            })
                            .collect(),
                    )
                } else {
                    unreachable!("non-json value")
                }
            }
        }
    }
    unsafe fn copy(&self, cx: &mut Cx, value: Dyn) -> R<Dyn> {
        if let Some(len) = self.array_len(cx, value) {
            let list = unsafe { List::<Dyn>::with_capacity(cx, self.list, len) }?;
            for i in 0..len {
                let v = unsafe { self.copy(cx, self.element(value, i)) }?;
                list.push(cx, v)?;
            }
            Ok(Dyn {
                tag: self.list as u64 + 3,
                payload: list.as_raw() as u64,
            })
        } else if let Some(len) = self.object_len(value) {
            let map = unsafe { Map::<Str, Dyn>::new(cx, self.map) }?;
            for i in 0..len {
                let (k, v) = self.entry(value, i);
                let v = unsafe { self.copy(cx, v) }?;
                map.set(cx, unsafe { Str::from_raw(k.payload as usize) }, v)?;
            }
            Ok(Dyn {
                tag: self.map as u64 + 3,
                payload: map.as_raw() as u64,
            })
        } else {
            Ok(value)
        }
    }
}
fn store_field(pointer: *mut u8, field: &crate::json_lists::Field, value: Dyn) {
    unsafe {
        let slot = pointer.add(field.offset);
        match field.slot {
            baml_rt_gc::SlotKind::Tagged => crate::write_dyn(pointer, slot.cast(), value),
            baml_rt_gc::SlotKind::Pointer => {
                baml_rt_gc::write_ptr(pointer, slot.cast(), value.payload as usize)
            }
            baml_rt_gc::SlotKind::Scalar => slot.cast::<u64>().write(value.payload),
        }
    }
}
unsafe fn fast(
    cx: &mut Cx,
    config: &Config,
    value: Dyn,
    ty: &Ty,
    build: bool,
    depth: usize,
) -> R<Option<Dyn>> {
    if depth > 128 {
        return Ok(None);
    }
    let result = match ty {
        Ty::Int if value.tag == 0 => Some(value),
        Ty::Float => {
            let number = match value.tag {
                0 => Some(value.payload as i64 as f64),
                crate::FLOAT_TAG => Some(f64::from_bits(value.payload)),
                tag if tag == config.bigint as u64 + 3 => {
                    unsafe { Str::from_raw(value.payload as usize) }
                        .as_str(cx)
                        .parse::<f64>()
                        .ok()
                        .filter(|f| f.is_finite())
                }
                _ => None,
            };
            number.map(|f| Dyn {
                tag: crate::FLOAT_TAG,
                payload: f.to_bits(),
            })
        }
        Ty::Bool if value.tag == 1 => Some(value),
        Ty::Null if value.tag == 2 => Some(value),
        Ty::String if value.tag == 3 => Some(value),
        Ty::Bigint { desc } => {
            if value.tag == config.bigint as u64 + 3 {
                Some(value)
            } else {
                let read_cx = unsafe { Cx::current() };
                let text = if value.tag == 0 {
                    Some((value.payload as i64).to_string())
                } else if value.tag == 3 {
                    Some(
                        unsafe { Str::from_raw(value.payload as usize) }
                            .as_str(&read_cx)
                            .to_owned(),
                    )
                } else {
                    None
                };
                if let Some(text) =
                    text.filter(|s| bex_vm_types::json_text::parse_decimal_bigint(s).is_some())
                {
                    Some(if build {
                        Dyn {
                            tag: *desc as u64 + 3,
                            payload: unsafe { crate::bigint::from_decimal(cx, *desc, &text) }?
                                .as_raw() as u64,
                        }
                    } else {
                        value
                    })
                } else {
                    None
                }
            }
        }
        Ty::Literal(w)
            if value.tag == 3
                && unsafe { Str::from_raw(value.payload as usize) }.as_str(cx) == *w =>
        {
            Some(value)
        }
        Ty::LiteralInt(w) if value.tag == 0 && value.payload as i64 == *w => Some(value),
        Ty::LiteralFloat(w) => {
            if let Some(v) = unsafe { fast(cx, config, value, &Ty::Float, false, depth + 1) }? {
                ((f64::from_bits(v.payload) - *w).abs() < f64::EPSILON).then_some(v)
            } else {
                None
            }
        }
        Ty::LiteralBool(w) if value.tag == 1 && (value.payload != 0) == *w => Some(value),
        Ty::Json(_) => Some(if build {
            unsafe { config.copy(cx, value) }?
        } else {
            value
        }),
        Ty::Union(members) => {
            let mut best = None;
            for member in *members {
                if unsafe { fast(cx, config, value, member.ty, false, depth + 1) }?.is_some() {
                    let score = match member.ty {
                        Ty::Literal(_)
                        | Ty::LiteralInt(_)
                        | Ty::LiteralFloat(_)
                        | Ty::LiteralBool(_) => usize::MAX,
                        Ty::Int if value.tag == 0 => 1,
                        Ty::Class { fields, .. } => {
                            let mut names = SmallVec::<[&str; 16]>::new();
                            for field in *fields {
                                if !field.skip
                                    && config.get(cx, value, field.name).is_some()
                                    && !names.contains(&field.name)
                                {
                                    names.push(field.name);
                                }
                            }
                            names.len()
                        }
                        _ => 0,
                    };
                    if best.is_none_or(|(_, old)| score > old) {
                        best = Some((member.ty, score));
                    }
                }
            }
            if let Some((ty, _)) = best {
                unsafe { fast(cx, config, value, ty, build, depth + 1) }?
            } else {
                None
            }
        }
        Ty::Class {
            desc,
            bytes,
            fields,
            ..
        } => {
            if config.object_len(value).is_none() || fields.iter().any(|f| f.skip) {
                return Ok(None);
            }
            let pointer = if build {
                unsafe { crate::try_alloc(*desc, *bytes) }?
            } else {
                std::ptr::null_mut()
            };
            for field in *fields {
                let item = match config.get(cx, value, field.name) {
                    Some(v) => v,
                    None if field.ty.nullable() => NULL,
                    _ => return Ok(None),
                };
                let Some(decoded) = unsafe { fast(cx, config, item, field.ty, build, depth + 1) }?
                else {
                    return Ok(None);
                };
                if build {
                    store_field(pointer, field, decoded);
                }
            }
            Some(if build {
                Dyn {
                    tag: *desc as u64 + 3,
                    payload: pointer as u64,
                }
            } else {
                value
            })
        }
        Ty::List {
            desc,
            element,
            slot,
        } => {
            let Some(len) = config.array_len(cx, value) else {
                return Ok(None);
            };
            if !build {
                for i in 0..len {
                    if unsafe {
                        fast(
                            cx,
                            config,
                            config.element(value, i),
                            element,
                            false,
                            depth + 1,
                        )
                    }?
                    .is_none()
                    {
                        return Ok(None);
                    }
                }
                Some(value)
            } else {
                unsafe fn fill<T: crate::lists::Slot>(
                    cx: &mut Cx,
                    config: &Config,
                    value: Dyn,
                    desc: u32,
                    element: &Ty,
                    len: usize,
                    depth: usize,
                    convert: impl Fn(Dyn) -> T,
                ) -> R<Option<usize>> {
                    let list = unsafe { List::<T>::with_capacity(cx, desc, len) }?;
                    for i in 0..len {
                        let Some(v) = unsafe {
                            fast(
                                cx,
                                config,
                                config.element(value, i),
                                element,
                                true,
                                depth + 1,
                            )
                        }?
                        else {
                            return Ok(None);
                        };
                        list.push(cx, convert(v))?;
                    }
                    Ok(Some(list.as_raw()))
                }
                let pointer = unsafe {
                    match slot {
                        baml_rt_gc::SlotKind::Tagged => {
                            fill::<Dyn>(cx, config, value, *desc, element, len, depth, |v| v)
                        }
                        baml_rt_gc::SlotKind::Pointer => fill::<crate::lists::Ptr>(
                            cx,
                            config,
                            value,
                            *desc,
                            element,
                            len,
                            depth,
                            |v| crate::lists::Ptr::from_raw(v.payload as usize),
                        ),
                        baml_rt_gc::SlotKind::Scalar => match element {
                            Ty::Float | Ty::LiteralFloat(_) => {
                                fill::<f64>(cx, config, value, *desc, element, len, depth, |v| {
                                    f64::from_bits(v.payload)
                                })
                            }
                            Ty::Bool | Ty::LiteralBool(_) => {
                                fill::<bool>(cx, config, value, *desc, element, len, depth, |v| {
                                    v.payload != 0
                                })
                            }
                            _ => fill::<i64>(cx, config, value, *desc, element, len, depth, |v| {
                                v.payload as i64
                            }),
                        },
                    }
                }?;
                pointer.map(|p| Dyn {
                    tag: *desc as u64 + 3,
                    payload: p as u64,
                })
            }
        }
        _ => None,
    };
    Ok(result)
}
/// # Safety
/// Config, schema and values describe live layouts in the active instance.
pub unsafe fn convert(cx: &mut Cx, config: &Config, value: Dyn, ty: &'static Ty) -> R<Dyn> {
    config.validate(cx, value)?;
    if let Some(result) = unsafe { fast(cx, config, value, ty, true, 0) }? {
        return Ok(result);
    }
    let owned = config.owned(cx, value);
    unsafe { crate::json_lists::from_owned(cx, ty, &owned) }
}
