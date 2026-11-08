//! Owner-thread UTF-8 strings: flat bytes, depth-one views, and lazy ropes.
use crate::{R, language::allocation_failure};
use baml_rt_gc::{Header, write_ptr};
use std::{cell::Cell, marker::PhantomData, rc::Rc};

const FLAT: u32 = 0;
const VIEW: u32 = 1;
const CONCAT: u32 = 2;
const INLINE_CAPACITY: usize = 54;

// Flat strings, views, and wide ropes share the variable-size descriptor.
// Common ropes fit in 32 bytes: their codepoint count uses the header's aux
// word. Counts beyond 31 bits retain the full-width representation.
pub const CONCAT_DESC_ID: u32 = 10;
pub const CONCAT_DESC: baml_rt_gc::Descriptor = baml_rt_gc::Descriptor {
    kind: baml_rt_gc::Kind::String,
    size: 32,
    ptr_slots: (8, 2),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 3,
    fields: &[],
    finalizer: None,
};
/// ASCII suffix runs store the suffix byte and total length in the header.
/// Persistent versions take one pointer and one header; repeated suffixes share
/// their original prefix rather than retaining intermediate versions.
pub const ASCII_APPEND_DESC_ID: u32 = 11;
pub const ASCII_APPEND_DESC: baml_rt_gc::Descriptor = baml_rt_gc::Descriptor {
    kind: baml_rt_gc::Kind::String,
    size: 16,
    ptr_slots: (8, 1),
    tagged_slots: (0, 0),
    elem: None,
    type_id: 3,
    fields: &[],
    finalizer: None,
};
#[repr(C)]
struct AsciiAppend {
    header: Header,
    left: usize,
}
const APPEND_FLAT: u32 = 1 << 31;
const APPEND_MAX: usize = (1 << 24) - 1;

pub(crate) fn compact_descriptor(descriptors: &[baml_rt_gc::Descriptor]) -> bool {
    descriptors.get(CONCAT_DESC_ID as usize).is_some_and(|d| {
        matches!(d.kind, baml_rt_gc::Kind::String)
            && d.size == 32
            && d.ptr_slots == (8, 2)
            && d.elem.is_none()
            && d.type_id == 3
    }) && descriptors
        .get(ASCII_APPEND_DESC_ID as usize)
        .is_some_and(|d| {
            matches!(d.kind, baml_rt_gc::Kind::String)
                && d.size == 16
                && d.ptr_slots == (8, 1)
                && d.elem.is_none()
                && d.type_id == 3
        })
}
thread_local! {pub(crate) static COMPACT_STRINGS:Cell<bool>=const{Cell::new(false)};}
#[repr(C)]
struct Compact {
    header: Header,
    left: usize,
    right: usize,
    bytes: usize,
}
const COMPACT_MAX: usize = (u32::MAX >> 1) as usize;

#[repr(C)]
struct Object {
    header: Header,
    cache: usize,
    left: usize,
    right: usize,
    bytes: usize,
    chars: usize,
}
const NODE: usize = std::mem::size_of::<Object>();
const BASE: usize = 80;
// Flat/view metadata follows the compact common header. Ropes allocate only
// NODE bytes and reuse the flattened object's hash when it exists.
const HASH: usize = 48;
const OFFSET: usize = 64;
const ASCII: usize = 72;

pub struct Cx {
    heap: *mut baml_rt_gc::Heap,
    roots: *const baml_rt_tasks::IntegrationRoots,
    compact: bool,
    _owner: PhantomData<Rc<()>>,
}
impl Cx {
    /// # Safety
    /// Handles must stay rooted within the installed owner-thread invocation.
    #[inline(always)]
    pub unsafe fn current() -> Self {
        let heap = crate::HEAP.get();
        let roots = crate::ROOTS.get();
        assert!(
            !heap.is_null() && !roots.is_null(),
            "builtin outside a root task"
        );
        Self {
            heap,
            roots,
            compact: COMPACT_STRINGS.get(),
            _owner: PhantomData,
        }
    }
    #[inline(always)]
    pub(crate) fn alloc(&self, desc: u32, bytes: usize) -> R<*mut u8> {
        // The invocation owns both pointers for this context's entire lifetime.
        // Construct mutable heap access only for the synchronous allocation.
        unsafe { (&mut *self.heap).try_alloc(desc, bytes, &crate::Roots(&*self.roots)) }
            .map_err(|failure| allocation_failure(failure.bytes))
    }
    pub fn poll(&mut self) -> R<()> {
        crate::language::poll()
    }
    pub fn string(&self, text: &str) -> R<Str> {
        let size = allocation_size(text.len())?;
        let pointer = self.alloc(0, size)?;
        unsafe { init_flat(pointer, text) };
        Ok(unsafe { Str::from_raw(pointer as usize) })
    }
}

pub(crate) fn allocation_size(len: usize) -> R<usize> {
    if len > baml_type::Int63::MAX.get() as usize {
        return Err(allocation_failure(len));
    }
    BASE.checked_add(len.next_multiple_of(8))
        .ok_or_else(|| allocation_failure(usize::MAX))
}
/// Initialize an allocated flat string without allocating or collecting.
pub(crate) unsafe fn init_flat(pointer: *mut u8, text: &str) {
    unsafe {
        let object = &mut *pointer.cast::<Object>();
        object.header.aux = FLAT;
        object.cache = 0;
        object.left = 0;
        object.right = 0;
        pointer.add(HASH).cast::<u128>().write(0);
        pointer.add(OFFSET).cast::<usize>().write(0);
        object.bytes = text.len();
        object.chars = text.chars().count();
        // The codepoint count already proves whether every byte is ASCII;
        // avoid a second scan on every flat-string allocation.
        pointer
            .add(ASCII)
            .cast::<usize>()
            .write(if object.chars == text.len() {
                text.len()
            } else {
                0
            });
        std::ptr::copy_nonoverlapping(text.as_ptr(), pointer.add(BASE), text.len());
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Str(usize, PhantomData<Rc<()>>);
impl Str {
    /// # Safety
    /// The pointer names a live string in this instance and follows Cx rooting.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self(pointer, PhantomData)
    }
    pub fn as_raw(self) -> usize {
        self.0
    }
    fn object(self) -> *mut Object {
        self.0 as *mut Object
    }
    #[inline(always)]
    fn append(self) -> bool {
        unsafe { (*(self.0 as *const Header)).desc == ASCII_APPEND_DESC_ID }
    }
    #[inline(always)]
    fn compact(self) -> bool {
        unsafe { (*(self.0 as *const Header)).desc == CONCAT_DESC_ID }
    }
    #[inline(always)]
    pub fn byte_len(self) -> usize {
        unsafe {
            if self.append() {
                (((*(self.0 as *const Header)).aux & !APPEND_FLAT) >> 7) as usize
            } else if self.compact() {
                (*(self.0 as *const Compact)).bytes
            } else {
                (*self.object()).bytes
            }
        }
    }
    #[inline(always)]
    pub fn char_len(self) -> usize {
        unsafe {
            if self.append() {
                (((*(self.0 as *const Header)).aux & !APPEND_FLAT) >> 7) as usize
            } else if self.compact() {
                ((*(self.0 as *const Header)).aux >> 1) as usize
            } else {
                (*self.object()).chars
            }
        }
    }
    fn ascii(self) -> usize {
        if self.byte_len() == self.char_len() {
            return self.byte_len();
        }
        unsafe {
            if self.compact() || (*self.object()).header.aux == CONCAT {
                0
            } else {
                (self.0 as *const u8).add(ASCII).cast::<usize>().read()
            }
        }
    }
    fn offset(self) -> usize {
        unsafe { (self.0 as *const u8).add(OFFSET).cast::<usize>().read() }
    }
    #[inline(always)]
    fn ascii_suffix(self) -> u8 {
        // Concats shorter than INLINE_CAPACITY are flat; a one-byte string
        // can therefore only be flat or a view, never an unflattened rope.
        unsafe {
            let object = self.object();
            debug_assert_eq!(self.byte_len(), 1);
            let address = if (*object).header.aux == VIEW {
                (*object).left + BASE + self.offset()
            } else {
                self.0 + BASE
            };
            (address as *const u8).read()
        }
    }
    fn flat_cache(self) -> Option<usize> {
        unsafe {
            let pointer = if self.append() {
                let node = &*(self.0 as *const AsciiAppend);
                if node.header.aux & APPEND_FLAT == 0 {
                    0
                } else {
                    node.left
                }
            } else if self.compact() {
                let node = &*(self.0 as *const Compact);
                if node.header.aux & 1 == 0 {
                    0
                } else {
                    node.left
                }
            } else if (*self.object()).header.aux == CONCAT {
                (*self.object()).cache
            } else {
                return None;
            };
            (pointer != 0).then_some(pointer)
        }
    }
    fn cached_hash(self) -> u128 {
        if self.append() || self.compact() || unsafe { (*self.object()).header.aux == CONCAT } {
            self.flat_cache().map_or(0, |p| unsafe {
                (p as *const u8).add(HASH).cast::<u128>().read()
            })
        } else {
            unsafe { (self.0 as *const u8).add(HASH).cast::<u128>().read() }
        }
    }
    fn flatten(self, cx: &Cx) -> R<usize> {
        unsafe {
            let bytes = self.byte_len();
            let chars = self.char_len();
            let ascii = self.ascii();
            let target = cx.alloc(0, allocation_size(bytes)?)?;
            init_flat(target, "");
            (*target.cast::<Object>()).bytes = bytes;
            (*target.cast::<Object>()).chars = chars;
            target.add(ASCII).cast::<usize>().write(ascii);
            let mut offset = 0;
            self.chunks(|part| {
                std::ptr::copy_nonoverlapping(part.as_ptr(), target.add(BASE + offset), part.len());
                offset += part.len();
            });
            Ok(target as usize)
        }
    }
    /// Flatten once, caching only a precisely traced managed pointer. Byte
    /// borrows remain valid across later allocations: this heap never moves.
    pub fn try_as_str<'a>(self, cx: &'a Cx) -> R<&'a str> {
        unsafe {
            let flat = if self.append() {
                match self.flat_cache() {
                    Some(flat) => flat,
                    None => {
                        let flat = self.flatten(cx)?;
                        let node = self.0 as *mut AsciiAppend;
                        write_ptr(node.cast(), &raw mut (*node).left, flat);
                        (*node).header.aux |= APPEND_FLAT;
                        flat
                    }
                }
            } else if self.compact() {
                match self.flat_cache() {
                    Some(flat) => flat,
                    None => {
                        let flat = self.flatten(cx)?;
                        let node = self.0 as *mut Compact;
                        write_ptr(node.cast(), &raw mut (*node).left, flat);
                        write_ptr(node.cast(), &raw mut (*node).right, 0);
                        (*node).header.aux |= 1;
                        flat
                    }
                }
            } else {
                let object = self.object();
                match (*object).header.aux {
                    FLAT => self.0,
                    VIEW => {
                        return Ok(std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                            ((*object).left as *const u8).add(BASE + self.offset()),
                            self.byte_len(),
                        )));
                    }
                    _ => match self.flat_cache() {
                        Some(flat) => flat,
                        None => {
                            let flat = self.flatten(cx)?;
                            write_ptr(object.cast(), &raw mut (*object).cache, flat);
                            write_ptr(object.cast(), &raw mut (*object).left, 0);
                            write_ptr(object.cast(), &raw mut (*object).right, 0);
                            flat
                        }
                    },
                }
            };
            Ok(std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                (flat as *const u8).add(BASE),
                self.byte_len(),
            )))
        }
    }
    /// Visit leaves iteratively; deep concatenation must not consume the fiber
    /// stack. The host work vector contains pointers only between non-GC calls.
    fn chunks(self, mut visit: impl FnMut(&str)) {
        enum Piece {
            String(Str),
            Ascii(u8, usize),
        }
        let mut work = vec![Piece::String(self)];
        while let Some(piece) = work.pop() {
            let Piece::String(value) = piece else {
                if let Piece::Ascii(byte, mut count) = piece {
                    let bytes = [byte; 128];
                    while count > 0 {
                        let chunk = count.min(bytes.len());
                        visit(unsafe { std::str::from_utf8_unchecked(&bytes[..chunk]) });
                        count -= chunk;
                    }
                }
                continue;
            };
            unsafe {
                if value.append() {
                    let node = &*(value.0 as *const AsciiAppend);
                    if node.header.aux & APPEND_FLAT == 0 {
                        let prefix = Self::from_raw(node.left);
                        work.push(Piece::Ascii(
                            (node.header.aux & 127) as u8,
                            value.byte_len() - prefix.byte_len(),
                        ));
                    }
                    work.push(Piece::String(Self::from_raw(node.left)));
                    continue;
                }
                if value.compact() {
                    let node = &*(value.0 as *const Compact);
                    if node.header.aux & 1 != 0 {
                        work.push(Piece::String(Self::from_raw(node.left)));
                    } else {
                        work.push(Piece::String(Self::from_raw(node.right)));
                        work.push(Piece::String(Self::from_raw(node.left)));
                    }
                    continue;
                }
                let object = value.object();
                match (*object).header.aux {
                    CONCAT if (*object).cache == 0 => {
                        work.push(Piece::String(Self::from_raw((*object).right)));
                        work.push(Piece::String(Self::from_raw((*object).left)));
                    }
                    CONCAT => work.push(Piece::String(Self::from_raw((*object).cache))),
                    kind => {
                        let (base, offset) = if kind == VIEW {
                            ((*object).left, value.offset())
                        } else {
                            (value.0, 0)
                        };
                        visit(std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                            (base as *const u8).add(BASE + offset),
                            value.byte_len(),
                        )));
                    }
                }
            }
        }
    }
    /// Owned snapshot for telemetry/host boundaries; never allocates on the
    /// managed heap or changes the language outcome if flattening would fail.
    pub fn to_owned(self) -> String {
        let mut text = String::with_capacity(self.byte_len());
        self.chunks(|part| text.push_str(part));
        text
    }
    #[cfg(test)]
    pub(super) fn as_str<'a>(&self, cx: &'a Cx) -> &'a str {
        self.try_as_str(cx).unwrap()
    }
}

#[inline(always)]
pub fn length(_cx: &Cx, value: Str) -> i64 {
    value.char_len() as i64
}

#[inline(always)]
pub fn concat(cx: &mut Cx, values: &[Str]) -> R<Str> {
    let Some((&first, rest)) = values.split_first() else {
        return cx.string("");
    };
    let mut result = first;
    for &right in rest {
        // The common persistent character-append case needs only the previous
        // header and the suffix byte. Avoid decoding both operands' general
        // rope layouts again for every version in a long append chain.
        if cx.compact
            && result.append()
            && unsafe { (*right.object()).header.desc == 0 && (*right.object()).bytes == 1 }
        {
            let count = result.byte_len();
            let suffix = right.ascii_suffix();
            if count < APPEND_MAX && suffix.is_ascii() {
                result = ascii_append(cx, result, count + 1, suffix)?;
                continue;
            }
        }
        if right.byte_len() == 0 {
            continue;
        }
        if result.byte_len() == 0 {
            result = right;
            continue;
        }
        let bytes = result
            .byte_len()
            .checked_add(right.byte_len())
            .filter(|n| *n <= baml_type::Int63::MAX.get() as usize)
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        // Like bex_str, short concatenations stay flat and inline in one object.
        if bytes <= INLINE_CAPACITY {
            let mut buffer = [0u8; INLINE_CAPACITY];
            let left = result.try_as_str(cx)?;
            let rhs = right.try_as_str(cx)?;
            buffer[..left.len()].copy_from_slice(left.as_bytes());
            buffer[left.len()..bytes].copy_from_slice(rhs.as_bytes());
            result = cx.string(unsafe { std::str::from_utf8_unchecked(&buffer[..bytes]) })?;
            continue;
        }
        let chars = result.char_len() + right.char_len();
        let pointer =
            if cx.compact && bytes == chars && bytes <= APPEND_MAX && right.byte_len() == 1 {
                result = ascii_append(cx, result, bytes, right.ascii_suffix())?;
                continue;
            } else if cx.compact && chars <= COMPACT_MAX {
                let pointer = cx.alloc(CONCAT_DESC_ID, 32)?;
                unsafe {
                    let node = &mut *pointer.cast::<Compact>();
                    node.header.aux = (chars as u32) << 1;
                    node.bytes = bytes;
                    write_ptr(pointer, &raw mut node.left, result.0);
                    write_ptr(pointer, &raw mut node.right, right.0);
                }
                pointer
            } else {
                let pointer = cx.alloc(0, NODE)?;
                unsafe {
                    let object = &mut *pointer.cast::<Object>();
                    object.header.aux = CONCAT;
                    object.bytes = bytes;
                    object.chars = chars;
                    write_ptr(pointer, &raw mut object.left, result.0);
                    write_ptr(pointer, &raw mut object.right, right.0);
                }
                pointer
            };
        result = unsafe { Str::from_raw(pointer as usize) };
    }
    Ok(result)
}

#[inline(always)]
fn ascii_append(cx: &Cx, left: Str, bytes: usize, suffix: u8) -> R<Str> {
    let prefix = if left.append()
        && unsafe { (*(left.0 as *const Header)).aux & (APPEND_FLAT | 127) == u32::from(suffix) }
    {
        unsafe { (*(left.0 as *const AsciiAppend)).left }
    } else {
        left.0
    };
    let pointer = cx.alloc(ASCII_APPEND_DESC_ID, 16)?;
    unsafe {
        let node = &mut *pointer.cast::<AsciiAppend>();
        node.header.aux = ((bytes as u32) << 7) | u32::from(suffix);
        write_ptr(pointer, &raw mut node.left, prefix);
        Ok(Str::from_raw(pointer as usize))
    }
}

fn byte_slice(cx: &mut Cx, value: Str, start: usize, end: usize) -> R<Str> {
    if start == 0 && end == value.byte_len() {
        return Ok(value);
    }
    let source = value.try_as_str(cx)?;
    let text = &source[start..end];
    let chars = text.chars().count();
    let ascii = if chars == text.len() { text.len() } else { 0 };
    let (parent, offset) = if let Some(flat) = value.flat_cache() {
        (flat, start)
    } else {
        unsafe {
            let object = value.object();
            match (*object).header.aux {
                FLAT => (value.0, start),
                VIEW => ((*object).left, value.offset() + start),
                _ => unreachable!("flattened string backing"),
            }
        }
    };
    // A slice of a slice must also account for the retained flat backing.
    if text.len() <= INLINE_CAPACITY && unsafe { Str::from_raw(parent) }.byte_len() > 4096 {
        return cx.string(text);
    }
    let pointer = cx.alloc(0, BASE)?;
    unsafe {
        let object = &mut *pointer.cast::<Object>();
        object.header.aux = VIEW;
        object.bytes = end - start;
        object.chars = chars;
        pointer.add(ASCII).cast::<usize>().write(ascii);
        pointer.add(OFFSET).cast::<usize>().write(offset);
        write_ptr(pointer, &raw mut object.left, parent);
    }
    Ok(unsafe { Str::from_raw(pointer as usize) })
}
fn char_byte(cx: &Cx, value: Str, index: usize) -> R<usize> {
    if index <= value.ascii() {
        return Ok(index);
    }
    Ok(value
        .try_as_str(cx)?
        .char_indices()
        .nth(index)
        .map_or(value.byte_len(), |(i, _)| i))
}
pub fn slice(cx: &mut Cx, value: Str, start: i64, end: i64) -> R<Str> {
    let start = baml_type::sequence::resolve_slice_bound(start, value.char_len());
    let end = baml_type::sequence::resolve_slice_bound(end, value.char_len()).max(start);
    let first = char_byte(cx, value, start)?;
    let last = char_byte(cx, value, end)?;
    byte_slice(cx, value, first, last)
}
pub fn integer_to_string(cx: &mut Cx, value: i64) -> R<Str> {
    cx.string(&value.to_string())
}
pub fn code_points(cx: &Cx, value: Str) -> R<Vec<i64>> {
    Ok(value
        .try_as_str(cx)?
        .chars()
        .map(|c| i64::from(u32::from(c)))
        .collect())
}
/// # Safety
/// `desc` describes a fixed int list. The result obeys Cx rooting.
pub unsafe fn to_code_points(cx: &mut Cx, value: Str, desc: u32) -> R<usize> {
    let values = code_points(cx, value)?;
    let bytes = values
        .len()
        .checked_mul(8)
        .and_then(|n| n.checked_add(16))
        .ok_or_else(|| allocation_failure(usize::MAX))?;
    let ptr = cx.alloc(desc, bytes)?;
    unsafe {
        ptr.add(8).cast::<usize>().write(values.len());
        std::ptr::copy_nonoverlapping(values.as_ptr(), ptr.add(16).cast(), values.len());
    }
    Ok(ptr as usize)
}

/// Shared Rust parsing rules and exactly the VM's diagnostic text.
pub fn parse_int(cx: &Cx, value: Str, error_desc: u32) -> R<i64> {
    let text = value.try_as_str(cx)?;
    let value = text.parse::<i64>().map_err(|e| {
        crate::language::panic_value(
            error_desc,
            "baml.errors.ParseError",
            format!("int.parse: cannot parse {text:?} as int: {e}"),
        )
    })?;
    if baml_type::Int63::new(value).is_none() {
        return Err(crate::language::panic_value(
            error_desc,
            "baml.errors.ParseError",
            format!(
                "int.parse: {text:?} is outside the representable BAML int range [{}, {}]",
                baml_type::Int63::MIN.get(),
                baml_type::Int63::MAX.get()
            ),
        ));
    }
    Ok(value)
}
/// # Safety
/// `values` names a rooted fixed-layout int list in the current instance.
pub unsafe fn from_code_points(cx: &mut Cx, values: usize, error_desc: u32) -> R<Str> {
    let len = unsafe { ((values + 8) as *const usize).read() };
    let mut text = String::with_capacity(len);
    for i in 0..len {
        let n = unsafe { ((values + 16) as *const i64).add(i).read() };
        let Some(cp) = u32::try_from(n).ok().and_then(char::from_u32) else {
            return Err(crate::language::panic_value(
                error_desc,
                "baml.errors.InvalidArgument",
                format!(
                    "string.from_code_points: value {n} at index {i} is not a valid Unicode code point (must be in [0, 0x10FFFF] and not a surrogate)"
                ),
            ));
        };
        text.push(cp);
    }
    cx.string(&text)
}
/// Cached content digest, identical to bex_str, independent of representation.
pub fn hash(cx: &Cx, value: Str) -> R<u128> {
    let hash = value.cached_hash();
    if hash != 0 {
        return Ok(hash);
    }
    let hash = xxhash_rust::xxh3::xxh3_128(value.try_as_str(cx)?.as_bytes());
    unsafe {
        let target = value.flat_cache().unwrap_or(value.0);
        (target as *mut u8).add(HASH).cast::<u128>().write(hash);
    };
    Ok(hash)
}
/// Equality at a nullable string boundary, without constructing null Str handles.
pub fn equals_raw(cx: &Cx, left: usize, right: usize) -> R<bool> {
    if left == right {
        return Ok(true);
    }
    if left == 0 || right == 0 {
        return Ok(false);
    }
    unsafe { equals(cx, Str::from_raw(left), Str::from_raw(right)) }
}
pub fn equals(cx: &Cx, left: Str, right: Str) -> R<bool> {
    if left.0 == right.0 {
        return Ok(true);
    }
    if left.byte_len() != right.byte_len() {
        return Ok(false);
    }
    let a = left.cached_hash();
    let b = right.cached_hash();
    if a != 0 && b != 0 && a != b {
        return Ok(false);
    }
    Ok(left.try_as_str(cx)? == right.try_as_str(cx)?)
}
pub fn byte_length(_cx: &Cx, value: Str) -> i64 {
    value.byte_len() as i64
}

pub fn transform(cx: &mut Cx, value: Str, method: &str) -> R<Str> {
    let source = value.try_as_str(cx)?;
    match method {
        "to_lower_case" => cx.string(&source.to_lowercase()),
        "to_upper_case" => cx.string(&source.to_uppercase()),
        _ => {
            let text = match method {
                "trim" => source.trim(),
                "trim_start" => source.trim_start(),
                "trim_end" => source.trim_end(),
                _ => unreachable!(),
            };
            let start = text.as_ptr() as usize - source.as_ptr() as usize;
            let end = start + text.len();
            byte_slice(cx, value, start, end)
        }
    }
}
pub fn predicate(cx: &Cx, value: Str, method: &str) -> R<bool> {
    Ok(value.try_as_str(cx)?.chars().all(|c| match method {
        "is_numeric" => c.is_numeric(),
        "is_alphabetic" => c.is_alphabetic(),
        "is_alphanumeric" => c.is_alphanumeric(),
        "is_uppercase" => c.is_uppercase(),
        "is_lowercase" => c.is_lowercase(),
        "is_whitespace" => c.is_whitespace(),
        "is_control" => c.is_control(),
        "is_graphic" => !c.is_control() && !c.is_whitespace(),
        "is_ascii" => c.is_ascii(),
        "is_ascii_numeric" => c.is_ascii_digit(),
        "is_ascii_alphabetic" => c.is_ascii_alphabetic(),
        "is_ascii_alphanumeric" => c.is_ascii_alphanumeric(),
        "is_ascii_uppercase" => c.is_ascii_uppercase(),
        "is_ascii_lowercase" => c.is_ascii_lowercase(),
        "is_ascii_whitespace" => c.is_ascii_whitespace(),
        "is_ascii_control" => c.is_ascii_control(),
        "is_ascii_graphic" => c.is_ascii_graphic(),
        "is_ascii_hex" => c.is_ascii_hexdigit(),
        _ => unreachable!(),
    }))
}
pub fn search(cx: &Cx, value: Str, needle: Str, method: &str) -> R<bool> {
    let text = value.try_as_str(cx)?;
    let needle = needle.try_as_str(cx)?;
    Ok(match method {
        "includes" => text.contains(needle),
        "starts_with" => text.starts_with(needle),
        "ends_with" => text.ends_with(needle),
        _ => unreachable!(),
    })
}
pub fn index_of(cx: &Cx, value: Str, needle: Str, last: bool) -> R<crate::Dyn> {
    let text = value.try_as_str(cx)?;
    let needle = needle.try_as_str(cx)?;
    let found = if last {
        text.rfind(needle)
    } else {
        text.find(needle)
    };
    Ok(
        found.map_or(crate::Dyn { tag: 2, payload: 0 }, |offset| crate::Dyn {
            tag: 0,
            payload: text[..offset].chars().count() as u64,
        }),
    )
}
fn resolve_index(index: i64, len: usize) -> Option<usize> {
    let index = if index < 0 { index + len as i64 } else { index };
    (index >= 0 && (index as usize) < len).then_some(index as usize)
}
pub fn at(cx: &mut Cx, value: Str, index: i64) -> R<usize> {
    let Some(index) = resolve_index(index, value.char_len()) else {
        return Ok(0);
    };
    Ok(slice(cx, value, index as i64, index as i64 + 1)?.as_raw())
}
pub fn code_point_at(cx: &Cx, value: Str, index: i64) -> R<crate::Dyn> {
    let Some(index) = resolve_index(index, value.char_len()) else {
        return Ok(crate::Dyn { tag: 2, payload: 0 });
    };
    let cp = value.try_as_str(cx)?.chars().nth(index).unwrap();
    Ok(crate::Dyn {
        tag: 0,
        payload: u64::from(u32::from(cp)),
    })
}
pub fn replace(cx: &mut Cx, value: Str, search: Str, replacement: Str, all: bool) -> R<Str> {
    let source = value.try_as_str(cx)?;
    let needle = search.try_as_str(cx)?;
    let replacement = replacement.try_as_str(cx)?;
    let text = if all {
        source.replace(needle, replacement)
    } else {
        source.replacen(needle, replacement, 1)
    };
    cx.string(&text)
}
pub fn repeat(cx: &mut Cx, value: Str, count: i64) -> R<Str> {
    let count = count.max(0) as usize;
    let bytes = value
        .byte_len()
        .checked_mul(count)
        .filter(|n| *n <= baml_type::Int63::MAX.get() as usize)
        .ok_or_else(|| allocation_failure(usize::MAX))?;
    if bytes == 0 {
        return cx.string("");
    }
    let mut base = value;
    let mut count = count;
    let mut result = cx.string("")?;
    while count > 0 {
        if count & 1 != 0 {
            result = concat(cx, &[result, base])?
        }
        count >>= 1;
        if count > 0 {
            base = concat(cx, &[base, base])?
        }
    }
    Ok(result)
}
/// # Safety
/// `desc` is a growing string-list descriptor pair. Results are independent
/// mutable lists; only immutable substring values may be shared.
pub unsafe fn segments(
    cx: &mut Cx,
    value: Str,
    delimiter: Option<Str>,
    lines: bool,
    desc: u32,
) -> R<usize> {
    // Copy the bound context to separate the source borrow from output allocation.
    let read_cx = Cx { ..*cx };
    let source = value.try_as_str(&read_cx)?;
    let owner = unsafe { Str::from_raw(value.flat_cache().unwrap_or(value.0)) };
    let cacheable =
        delimiter.is_some_and(|d| d.byte_len() > 0 && d.byte_len() <= 128) && source.len() <= 65536;
    if cacheable {
        let cached = unsafe { (*owner.object()).cache };
        if cached != 0 {
            let metadata = cached as *const Object;
            let key = unsafe { Str::from_raw((*metadata).left) };
            if unsafe { (*metadata).bytes } == desc as usize
                && equals(&read_cx, key, delimiter.unwrap())?
            {
                let snapshot = unsafe { crate::lists::List::<Str>::from_raw((*metadata).right) };
                return Ok(snapshot.copy(cx)?.as_raw());
            }
        }
    }
    let list = unsafe { crate::lists::List::<Str>::new(cx, desc) }?;
    let mut push = |part: &str| -> R<()> {
        let start = part.as_ptr() as usize - source.as_ptr() as usize;
        let view = byte_slice(cx, value, start, start + part.len())?;
        list.push(cx, view)?;
        Ok(())
    };
    if lines {
        for part in source.lines() {
            push(part)?
        }
    } else if let Some(delimiter) = delimiter {
        let needle = delimiter.try_as_str(&read_cx)?;
        if needle.is_empty() {
            for (start, c) in source.char_indices() {
                push(&source[start..start + c.len_utf8()])?
            }
        } else {
            for part in source.split(needle) {
                push(part)?
            }
        }
    } else {
        for (start, c) in source.char_indices() {
            push(&source[start..start + c.len_utf8()])?
        }
    }
    // Bounded one-entry cache, owned by the string and traced with it. Every
    // caller gets independent list storage; mutation cannot poison the cache.
    if cacheable && list.len(cx) <= 64 {
        if let Ok(snapshot) = list.copy(cx) {
            if let Ok(metadata) = cx.alloc(0, NODE) {
                unsafe {
                    let object = &mut *metadata.cast::<Object>();
                    object.header.aux = 3;
                    object.bytes = desc as usize;
                    write_ptr(metadata, &raw mut object.left, delimiter.unwrap().0);
                    write_ptr(metadata, &raw mut object.right, snapshot.as_raw());
                    write_ptr(
                        owner.object().cast(),
                        &raw mut (*owner.object()).cache,
                        metadata as usize,
                    );
                }
            }
        }
    }
    Ok(list.as_raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*};
    static DESCRIPTORS: &[baml_rt_gc::Descriptor] = &[
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
        CONCAT_DESC,
        ASCII_APPEND_DESC,
    ];
    #[test]
    fn unicode_negative_slices_and_concat_survive_collecting_allocations() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let value = cx.string("aé🦀z")?;
            assert_eq!(length(&cx, value), 4);
            assert_eq!(code_points(&cx, value)?, [97, 233, 129408, 122]);
            for start in [i64::MIN, -6, -4, -1, 0, 1, 4, 6, i64::MAX] {
                for end in [i64::MIN, -6, -4, -1, 0, 1, 4, 6, i64::MAX] {
                    let result = slice(&mut cx, value, start, end)?;
                    let first = baml_type::sequence::resolve_slice_bound(start, 4);
                    let last = baml_type::sequence::resolve_slice_bound(end, 4).max(first);
                    let expected: String = "aé🦀z".chars().skip(first).take(last - first).collect();
                    assert_eq!(result.as_str(&cx), expected);
                    unsafe { crate::safepoint() }.unwrap();
                }
            }
            let number = integer_to_string(&mut cx, -42)?;
            let result = concat(&mut cx, &[value, number])?;
            assert_eq!(result.as_str(&cx), "aé🦀z-42");
            Ok(4)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 4);
        assert!(instance.stats().collections >= 81);
    }
    #[test]
    fn deep_ropes_flatten_iteratively_and_keep_aliases_and_cached_hashes() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let leaf = cx.string("é🦀")?;
            let mut rope = cx.string("")?;
            for _ in 0..12000 {
                rope = concat(&mut cx, &[rope, leaf])?;
            }
            assert_eq!(length(&cx, rope), 24000);
            let expected = "é🦀".repeat(12000);
            let mut capture = crate::telemetry::capture::Scratch::default();
            let actual = unsafe {
                capture.value(
                    &[crate::telemetry::capture::Shape::String],
                    crate::Dyn {
                        tag: 3,
                        payload: rope.as_raw() as u64,
                    },
                )
            };
            assert_eq!(actual.root_id(), capture.text(&expected).root_id());
            assert!(
                rope.flat_cache().is_none(),
                "capture must not flatten on the managed heap"
            );
            assert!(rope.compact());
            let view = slice(&mut cx, rope, 1, 45)?;
            assert_eq!(unsafe { (*view.object()).header.aux }, VIEW);
            let captured = unsafe {
                capture.value(
                    &[crate::telemetry::capture::Shape::String],
                    crate::Dyn {
                        tag: 3,
                        payload: view.as_raw() as u64,
                    },
                )
            };
            assert_eq!(
                captured.root_id(),
                capture.text(&"🦀é".repeat(22)).root_id()
            );
            let alias = slice(&mut cx, rope, 23996, 24000)?;
            assert_eq!(alias.as_str(&cx), "é🦀é🦀");
            let expected = "é🦀".repeat(12000);
            assert_eq!(rope.to_owned(), expected);
            let first = rope.try_as_str(&cx)?.as_ptr();
            assert_eq!(rope.try_as_str(&cx)?.as_ptr(), first);
            let flat = cx.string(&expected)?;
            assert!(equals(&cx, rope, flat)?);
            assert_eq!(hash(&cx, rope)?, hash(&cx, flat)?);
            assert_eq!(
                hash(&cx, rope)?,
                xxhash_rust::xxh3::xxh3_128(expected.as_bytes())
            );
            Ok(length(&cx, rope))
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true,
            ],
        );
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 24000);
    }
    #[test]
    fn ascii_append_versions_remain_immutable_across_gc_and_flattening() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let leaf = cx.string("x")?;
            let mut text = cx.string(&"a".repeat(80))?;
            let mut alias = text;
            for i in 0..160 {
                text = concat(&mut cx, &[text, leaf])?;
                if i == 79 {
                    alias = text;
                }
                cx.poll()?;
            }
            assert!(text.append());
            assert_eq!(text.char_len(), 240);
            let expected = format!("{}{}", "a".repeat(80), "x".repeat(160));
            assert_eq!(text.to_owned(), expected);
            assert!(text.flat_cache().is_none());
            let view = slice(&mut cx, text, 75, 85)?;
            assert_eq!(view.to_owned(), "aaaaaxxxxx");
            let flat = cx.string(&expected)?;
            assert!(equals(&cx, text, flat)?);
            assert_eq!(hash(&cx, text)?, hash(&cx, flat)?);
            cx.poll()?;
            assert_eq!(
                alias.to_owned(),
                format!("{}{}", "a".repeat(80), "x".repeat(80))
            );
            assert_eq!(text.try_as_str(&cx)?, expected);
            Ok(240)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 240);
    }
    #[test]
    fn views_unicode_methods_and_snapshot_survive_stress() {
        static D: &[baml_rt_gc::Descriptor] = &[
            STRING_DESC,
            OVERFLOW_DESC,
            SHIFT_DESC,
            CANCEL_DESC,
            STACK_DESC,
            ALLOC_DESC,
            crate::lists::descriptor(9),
            crate::lists::buffer_descriptor::<Str>(10),
        ];
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let text = cx.string("  é🦀|Σß|last|  ")?;
            let trimmed = transform(&mut cx, text, "trim")?;
            let separator = cx.string("|")?;
            let list = unsafe {
                crate::lists::List::<Str>::from_raw(segments(
                    &mut cx,
                    trimmed,
                    Some(separator),
                    false,
                    6,
                )?)
            };
            assert_eq!(list.len(&cx), 4);
            let extra = cx.string("extra")?;
            list.push(&mut cx, extra)?;
            let fresh = unsafe {
                crate::lists::List::<Str>::from_raw(segments(
                    &mut cx,
                    trimmed,
                    Some(separator),
                    false,
                    6,
                )?)
            };
            assert_eq!(fresh.len(&cx), 4);
            for _ in 0..20 {
                let again = unsafe {
                    crate::lists::List::<Str>::from_raw(segments(
                        &mut cx,
                        trimmed,
                        Some(separator),
                        false,
                        6,
                    )?)
                };
                assert_eq!(again.len(&cx), 4);
                assert_eq!(again.get(&cx, 0)?.as_str(&cx), "é🦀");
                cx.poll()?;
            }

            assert_eq!(list.get(&cx, 0)?.as_str(&cx), "é🦀");
            let second = list.get(&cx, 1)?;
            assert_eq!(
                transform(&mut cx, second, "to_upper_case")?.as_str(&cx),
                "ΣSS"
            );
            let sigma = cx.string("ΟΣ")?;
            assert_eq!(
                transform(&mut cx, sigma, "to_lower_case")?.as_str(&cx),
                "ος"
            );
            let needle = cx.string("🦀")?;
            assert_eq!(index_of(&cx, trimmed, needle, false)?.payload, 1);
            assert_eq!(
                unsafe { Str::from_raw(at(&mut cx, trimmed, -1)?) }.as_str(&cx),
                "|"
            );
            assert_eq!(at(&mut cx, trimmed, i64::MIN)?, 0);
            assert_eq!(code_point_at(&cx, trimmed, 1)?.payload, 129408);
            assert_eq!(code_point_at(&cx, trimmed, i64::MAX)?.tag, 2);
            let dollars = cx.string("$1")?;
            assert_eq!(
                replace(&mut cx, trimmed, separator, dollars, false)?.as_str(&cx),
                "é🦀$1Σß|last|"
            );
            let empty = cx.string("")?;
            let chars = unsafe {
                crate::lists::List::<Str>::from_raw(segments(
                    &mut cx,
                    trimmed,
                    Some(empty),
                    false,
                    6,
                )?)
            };
            assert_eq!(chars.len(&cx), trimmed.char_len() as i64);
            let ends = cx.string("a\r\nb\r\nc\r\n")?;
            let lines = unsafe {
                crate::lists::List::<Str>::from_raw(segments(&mut cx, ends, None, true, 6)?)
            };
            assert_eq!(lines.len(&cx), 3);
            let large = cx.string(&"x".repeat(8192))?;
            let tiny = slice(&mut cx, large, 5, 8)?;
            assert_eq!(unsafe { (*tiny.object()).header.aux }, FLAT);
            let view = slice(&mut cx, large, 100, 300)?;
            let nested = slice(&mut cx, view, 25, 100)?;
            assert_eq!(unsafe { (*nested.object()).left }, large.0);
            assert_eq!(nested.offset(), 125);
            let rope = concat(&mut cx, &[text, large])?;
            cx.poll()?;
            assert_eq!(
                rope.to_owned(),
                format!("{}{}", text.to_owned(), "x".repeat(8192))
            );
            assert_eq!(list.get(&cx, 0)?.as_str(&cx), "é🦀");
            assert_eq!(repeat(&mut cx, second, 3)?.as_str(&cx), "ΣßΣßΣß");
            assert!(predicate(&cx, empty, "is_uppercase")?);
            assert!(!predicate(&cx, second, "is_ascii")?);
            Ok(1)
        }
        let mut instance = Instance::new(
            D,
            &[
                false, false, false, true, true, true, true, true, true, true, true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
    }
    #[test]
    fn failed_flatten_is_a_language_allocation_error_and_preserves_rope() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let leaf = cx.string(&"a".repeat(1024))?;
            let rope = repeat(&mut cx, leaf, 128)?;
            let error = rope.try_as_str(&cx).unwrap_err();
            assert_eq!(error.class, "baml.panics.AllocFailure");
            assert_eq!(rope.char_len(), 131072);
            assert_eq!(rope.to_owned(), "a".repeat(131072));
            Ok(1)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true,
            ],
        );
        instance.set_heap_limit(32768);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
    }
    #[test]
    fn compact_counts_promote_without_truncation_or_flattening() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let unit = cx.string("a")?;
            let compact = repeat(&mut cx, unit, COMPACT_MAX as i64)?;
            assert!(compact.compact());
            assert_eq!(compact.char_len(), COMPACT_MAX);
            let wide = concat(&mut cx, &[compact, unit])?;
            assert!(!wide.compact());
            assert_eq!(wide.char_len(), COMPACT_MAX + 1);
            let huge = repeat(&mut cx, unit, 1i64 << 32)?;
            assert_eq!(huge.char_len(), 1usize << 32);
            assert_eq!(huge.byte_len(), 1usize << 32);
            let snapshot = unsafe {
                crate::telemetry::capture::Scratch::default().value(
                    &[crate::telemetry::capture::Shape::String],
                    crate::Dyn {
                        tag: 3,
                        payload: huge.0 as u64,
                    },
                )
            };
            assert!(matches!(
                snapshot.root(),
                btel_snapshot::SnapshotRoot::Value(btel_snapshot::SnapshotValue::Truncated(
                    btel_snapshot::Limit::Bytes
                ))
            ));
            assert!(huge.flat_cache().is_none());
            assert!(huge.try_as_str(&cx).is_err());
            assert_eq!(huge.char_len(), 1usize << 32);
            Ok(1)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true,
            ],
        );
        instance.set_heap_limit(32768);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
    }
}
