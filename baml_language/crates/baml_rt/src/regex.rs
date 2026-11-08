//! Regex adapters share the VM's heap-neutral engines and offset rules.
use crate::{
    Dyn, Error, R,
    strings::{Cx, Str},
};
use sys_regex::{BuildError, Program, RawMatch, SearchAborted};

pub struct ClassLayout {
    pub desc: u32,
    pub size: usize,
    pub fields: &'static [usize],
}
impl ClassLayout {
    fn new(&self, cx: &mut Cx) -> R<usize> {
        Ok(cx.alloc(self.desc, self.size)? as usize)
    }
    unsafe fn pointer(&self, object: usize, index: usize, value: usize) {
        unsafe {
            baml_rt_gc::write_ptr(
                object as *mut u8,
                (object + self.fields[index]) as *mut usize,
                value,
            )
        }
    }
    unsafe fn scalar(&self, object: usize, index: usize, value: u64) {
        unsafe { ((object + self.fields[index]) as *mut u64).write(value) }
    }
    unsafe fn tagged(&self, object: usize, index: usize, value: Dyn) {
        unsafe {
            crate::write_dyn(
                object as *mut u8,
                (object + self.fields[index]) as *mut Dyn,
                value,
            )
        }
    }
}
pub struct Schema {
    pub regex: ClassLayout,
    pub group: ClassLayout,
    pub found: ClassLayout,
    pub error: ClassLayout,
    pub error_kinds: &'static [&'static str],
    pub panic_desc: u32,
    pub groups_desc: u32,
    pub matches_desc: u32,
    pub named_desc: u32,
}
unsafe fn program(regex: usize, schema: &Schema) -> std::rc::Rc<Program> {
    let handle = unsafe { ((regex + schema.regex.fields[0]) as *const usize).read() };
    unsafe { crate::native::get(handle) }
}
fn aborted(program: &Program, error: SearchAborted, desc: u32) -> Box<Error> {
    crate::language::panic_value(
        desc,
        "baml.panics.UserPanic",
        format!(
            "baml.regex: {} while matching /{}/ — this pattern was compiled with `backtracking = true`, whose matching time is not bounded",
            error.reason,
            program.pattern()
        ),
    )
}
pub fn compile(cx: &mut Cx, pattern: Str, backtracking: bool, schema: &Schema) -> R<usize> {
    let read = unsafe { Cx::current() };
    let text = pattern.try_as_str(&read)?;
    match Program::compile(text, backtracking) {
        Ok(program) => {
            // Program owns only engine state and offsets, never managed values.
            let handle = unsafe { crate::native::new(cx, program)? };
            let value = schema.regex.new(cx)?;
            unsafe { schema.regex.pointer(value, 0, handle) };
            Ok(value)
        }
        Err(error) => Err(build_error(cx, pattern, &error, schema)?),
    }
}
fn build_error(cx: &mut Cx, pattern: Str, error: &BuildError, schema: &Schema) -> R<Box<Error>> {
    let read = unsafe { Cx::current() };
    let text = pattern.try_as_str(&read)?;
    let kind = schema
        .error_kinds
        .iter()
        .position(|name| *name == error.kind.variant_name())
        .expect("regex error kind");
    let spans = error
        .span
        .map(|(start, end)| sys_regex::char_offsets(text, &[start, end]));
    let start = spans.as_ref().map(|s| s[0] as i64);
    let end = spans.as_ref().map(|s| s[1] as i64);
    let value = schema.error.new(cx)?;
    unsafe {
        schema.error.scalar(value, 0, kind as u64);
        schema
            .error
            .pointer(value, 1, cx.string(&error.message)?.as_raw());
        schema.error.pointer(value, 2, pattern.as_raw());
        for (i, offset) in [(3, start), (4, end)] {
            schema.error.tagged(
                value,
                i,
                offset.map_or(Dyn { tag: 2, payload: 0 }, |n| Dyn {
                    tag: 0,
                    payload: n as u64,
                }),
            );
        }
    }
    let offset = |n: Option<i64>| n.map_or_else(|| "null".to_owned(), |n| n.to_string());
    Ok(Box::new(Error {
        telemetry_caught: None,
        telemetry_at: None,
        telemetry_raise: None,
        value,
        class: "baml.regex.Error",
        message: error.message.clone(),
        frames: vec![],
        display_value: Some(format!(
            "baml.regex.Error {{kind: {}, message: {:?}, pattern: {:?}, span_start: {}, span_end: {}}}",
            error.kind.variant_name(),
            error.message,
            text,
            offset(start),
            offset(end)
        )),
    }))
}
pub fn escape(cx: &mut Cx, literal: Str) -> R<Str> {
    let read = unsafe { Cx::current() };
    cx.string(&sys_regex::escape(literal.try_as_str(&read)?))
}
/// # Safety
/// `regex` is a rooted Regex matching schema's layout in the current instance.
pub unsafe fn has_match(cx: &Cx, regex: usize, haystack: Str, schema: &Schema) -> R<bool> {
    let program = unsafe { program(regex, schema) };
    program
        .is_match(haystack.try_as_str(cx)?)
        .map_err(|e| aborted(&program, e, schema.panic_desc))
}
/// # Safety
/// Same regex layout and rooting contract as `has_match`.
pub unsafe fn replace(
    cx: &mut Cx,
    regex: usize,
    haystack: Str,
    template: Str,
    all: bool,
    schema: &Schema,
) -> R<Str> {
    let program = unsafe { program(regex, schema) };
    let read = unsafe { Cx::current() };
    let result = program
        .replacen(
            haystack.try_as_str(&read)?,
            if all { 0 } else { 1 },
            template.try_as_str(&read)?,
        )
        .map_err(|e| aborted(&program, e, schema.panic_desc))?;
    match result {
        std::borrow::Cow::Borrowed(_) => Ok(haystack),
        std::borrow::Cow::Owned(text) => cx.string(&text),
    }
}
/// # Safety
/// Same regex layout and rooting contract as `has_match`; list_desc is string[].
pub unsafe fn split(
    cx: &mut Cx,
    regex: usize,
    haystack: Str,
    list_desc: u32,
    schema: &Schema,
) -> R<usize> {
    let program = unsafe { program(regex, schema) };
    let read = unsafe { Cx::current() };
    let spans = program
        .split(haystack.try_as_str(&read)?)
        .map_err(|e| aborted(&program, e, schema.panic_desc))?;
    let list = unsafe { crate::lists::List::<Str>::new(cx, list_desc)? };
    for (start, end) in spans {
        let text = crate::strings::byte_slice(cx, haystack, start, end)?;
        list.push(cx, text)?;
    }
    Ok(list.as_raw())
}
unsafe fn pointer_list(cx: &mut Cx, desc: u32, length: usize) -> R<usize> {
    let list = unsafe { crate::lists::List::<crate::lists::Ptr>::with_capacity(cx, desc, length) }?;
    for _ in 0..length {
        list.push(cx, unsafe { crate::lists::Ptr::from_raw(0) })?;
    }
    Ok(list.as_raw())
}
unsafe fn list_pointer(list: usize, index: usize, value: usize) {
    let cx = unsafe { Cx::current() };
    unsafe { crate::lists::List::<crate::lists::Ptr>::from_raw(list) }
        .set(&cx, index as i64, unsafe {
            crate::lists::Ptr::from_raw(value)
        })
        .expect("preallocated regex list index");
}
fn materialize(
    cx: &mut Cx,
    haystack: Str,
    raw: &RawMatch,
    names: &[Option<String>],
    chars: &[usize],
    schema: &Schema,
) -> R<usize> {
    let groups = unsafe { pointer_list(cx, schema.groups_desc, raw.groups.len())? };
    // VM regex groups deliberately carry unknown as their runtime element type.
    unsafe {
        (*(groups as *mut baml_rt_gc::Header)).aux = crate::telemetry::capture::UNKNOWN_ELEMENT_TYPE
    };
    let named = unsafe { crate::regex_map::StringPtrMap::new(cx, schema.named_desc)? };
    unsafe {
        (*(named.as_raw() as *mut baml_rt_gc::Header)).aux =
            crate::telemetry::capture::UNKNOWN_ELEMENT_TYPE;
    }
    let mut cursor = 0;
    for (index, span) in raw.groups.iter().enumerate() {
        if let Some((start, end)) = *span {
            let group = schema.group.new(cx)?;
            let text = crate::strings::byte_slice(cx, haystack, start, end)?;
            unsafe {
                schema.group.pointer(group, 0, text.as_raw());
                schema.group.scalar(group, 1, chars[cursor] as u64);
                schema.group.scalar(group, 2, chars[cursor + 1] as u64);
                list_pointer(groups, index, group)
            };
            cursor += 2;
        }
        if let Some(name) = names.get(index).and_then(Option::as_deref) {
            let key = cx.string(name)?;
            let group = unsafe { crate::lists::List::<crate::lists::Ptr>::from_raw(groups) }
                .get(cx, index as i64)?
                .as_raw();
            named.set(cx, key, group)?;
        }
    }
    let value = schema.found.new(cx)?;
    let (start, end) = raw.span();
    let text = crate::strings::byte_slice(cx, haystack, start, end)?;
    unsafe {
        schema.found.pointer(value, 0, text.as_raw());
        schema
            .found
            .scalar(value, 1, chars.first().copied().unwrap_or(0) as u64);
        schema
            .found
            .scalar(value, 2, chars.get(1).copied().unwrap_or(0) as u64);
        schema.found.pointer(value, 3, groups);
        schema.found.pointer(value, 4, named.as_raw());
    }
    Ok(value)
}
/// # Safety
/// Same regex layout/rooting contract as has_match. All native temporary spans
/// are heap-neutral; each managed match is linked into the traced result before
/// any later match is allocated.
pub unsafe fn matches(
    cx: &mut Cx,
    regex: usize,
    haystack: Str,
    mode: &str,
    schema: &Schema,
) -> R<usize> {
    let program = unsafe { program(regex, schema) };
    let read = unsafe { Cx::current() };
    let text = haystack.try_as_str(&read)?;
    let found = match mode {
        "match_once" => program.find_first(text).map(|v| v.into_iter().collect()),
        "match_full" => program.find_exact(text).map(|v| v.into_iter().collect()),
        "match_all" => program.find_all(text),
        _ => unreachable!(),
    }
    .map_err(|e| aborted(&program, e, schema.panic_desc))?;
    let spans: Vec<_> = found
        .iter()
        .flat_map(|m| {
            m.groups
                .iter()
                .flatten()
                .flat_map(|&(start, end)| [start, end])
        })
        .collect();
    let offsets = sys_regex::char_offsets(text, &spans);
    let mut remaining = offsets.as_slice();
    let list = unsafe { pointer_list(cx, schema.matches_desc, found.len())? };
    unsafe {
        (*(list as *mut baml_rt_gc::Header)).aux = crate::telemetry::capture::UNKNOWN_ELEMENT_TYPE;
    }
    for (index, raw) in found.iter().enumerate() {
        let count = raw.groups.iter().flatten().count() * 2;
        let (chars, rest) = remaining.split_at(count);
        remaining = rest;
        let value = materialize(cx, haystack, raw, program.names(), chars, schema)?;
        unsafe { list_pointer(list, index, value) }
    }
    Ok(if mode == "match_all" {
        list
    } else if found.is_empty() {
        0
    } else {
        unsafe { crate::lists::List::<crate::lists::Ptr>::from_raw(list) }
            .get(cx, 0)?
            .as_raw()
    })
}
pub fn index_from(cx: &Cx, haystack: Str, search: Str, start: i64) -> R<Dyn> {
    let haystack = bex_str::BexStr::from(haystack.try_as_str(cx)?);
    let search = search.try_as_str(cx)?;
    let found = usize::try_from(start)
        .ok()
        .and_then(|start| haystack.char_find_from(search, start));
    Ok(found.map_or(Dyn { tag: 2, payload: 0 }, |i| Dyn {
        tag: 0,
        payload: i as u64,
    }))
}
