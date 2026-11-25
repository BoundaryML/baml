//! Strings: immutable UTF-8 with a cached count of characters, so that
//! `length()` is a load.

use crate::{
    heap::{Mutator, Value, load, store},
    layout::{
        SLICE_OFFSET, SLICE_PARENT, STRING_CHAR_COUNT_SHIFT, STRING_LENGTHS, STRING_TAGS_END,
        STRING_TEXT, TAG_BOOL, TAG_FLOAT, TAG_INT, TAG_NULL, TAG_STRING, TYPE_ID_MASK, VALUE, WORD,
        string_size,
    },
    native::View as _,
};

/// The text of the string object at `string`: the one place that tells a
/// string holding its text from a slice of another.
///
/// # Safety
///
/// `string` is a live string object, and the result is not used after the
/// next allocation.
#[inline]
pub unsafe fn text<'a>(string: usize) -> &'a str {
    // SAFETY: a string object holds its byte length and then either that
    // many bytes of UTF-8 or where they are in another string, which holds
    // its own. An empty string that holds its text ends at its lengths, so
    // a slice's words are read of a slice only.
    unsafe {
        let length = load(string + STRING_LENGTHS) & 0xFFFF_FFFF;
        let start = if load(string) & TYPE_ID_MASK == TAG_STRING {
            string + STRING_TEXT
        } else {
            load(string + SLICE_PARENT) + STRING_TEXT + load(string + SLICE_OFFSET)
        };
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(start as *const u8, length))
    }
}

/// Lay out a string of `bytes` bytes and `chars` characters at `string`,
/// with its text left to be written and the bytes after it zero.
///
/// # Safety
///
/// `string` is `string_size(bytes)` fresh bytes.
pub(crate) unsafe fn format(string: usize, bytes: usize, chars: usize) {
    assert!(
        u32::try_from(bytes).is_ok(),
        "a string cannot be longer than 4 GiB"
    );
    // SAFETY: the caller's contract. The end first, for the zeros after the
    // text: whole words of it are hashed and compared. A string with no
    // text ends at its lengths, which are written after.
    unsafe {
        *((string + string_size(bytes) - VALUE) as *mut Value) = Value::NULL;
        store(string, TAG_STRING);
        store(
            string + STRING_LENGTHS,
            bytes | (chars << STRING_CHAR_COUNT_SHIFT),
        );
    }
}

/// Allocate a string of `bytes` bytes and `chars` characters with its text
/// left to be written. May collect.
fn allocate(mutator: &mut Mutator, bytes: usize, chars: usize) -> usize {
    let string = mutator.allocate(string_size(bytes));
    // SAFETY: the room the string takes.
    unsafe { format(string, bytes, chars) };
    string
}

/// A new string holding the text of the string in the root `source`. May
/// collect.
///
/// # Safety
///
/// `source` is a root holding a live string.
pub(crate) unsafe fn copy_flat(mutator: &mut Mutator, source: *const usize) -> usize {
    // SAFETY: the caller's contract: the root is read again after the
    // allocation, which can move the string.
    unsafe {
        let lengths = load(source.read_volatile() + STRING_LENGTHS);
        let bytes = lengths & 0xFFFF_FFFF;
        let copy = allocate(mutator, bytes, lengths >> STRING_CHAR_COUNT_SHIFT);
        let from = text(source.read_volatile());
        std::ptr::copy_nonoverlapping(from.as_ptr(), (copy + STRING_TEXT) as *mut u8, bytes);
        copy
    }
}

/// A new string object holding `value`. May collect.
pub fn from_str(mutator: &mut Mutator, value: &str) -> usize {
    let string = allocate(mutator, value.len(), value.chars().count());
    // SAFETY: the object has room for `value.len()` bytes of text.
    unsafe {
        std::ptr::copy_nonoverlapping(
            value.as_ptr(),
            (string + STRING_TEXT) as *mut u8,
            value.len(),
        );
    }
    string
}

/// A new string object holding `value`, every character of which is one
/// byte. May collect.
fn from_ascii(mutator: &mut Mutator, value: &str) -> usize {
    debug_assert!(value.is_ascii());
    let string = allocate(mutator, value.len(), value.len());
    // SAFETY: the object has room for `value.len()` bytes of text.
    unsafe {
        std::ptr::copy_nonoverlapping(
            value.as_ptr(),
            (string + STRING_TEXT) as *mut u8,
            value.len(),
        );
    }
    string
}

/// The most characters the decimal text of an `int` has.
pub const INT_DIGITS: usize = 20;

/// The decimal text of `value`, written at the end of `buffer`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "what is narrowed is under a hundred"
)]
pub fn int_digits(value: i64, buffer: &mut [u8; INT_DIGITS]) -> &str {
    const PAIRS: &[u8; 200] = b"00010203040506070809101112131415161718192021222324252627282930313233343536373839404142434445464748495051525354555657585960616263646566676869707172737475767778798081828384858687888990919293949596979899";
    let mut rest = value.unsigned_abs();
    let mut at = INT_DIGITS;
    while rest >= 100 {
        let pair = (rest % 100) as usize * 2;
        rest /= 100;
        at -= 2;
        buffer[at..at + 2].copy_from_slice(&PAIRS[pair..pair + 2]);
    }
    if rest >= 10 {
        let pair = rest as usize * 2;
        at -= 2;
        buffer[at..at + 2].copy_from_slice(&PAIRS[pair..pair + 2]);
    } else {
        at -= 1;
        buffer[at] = b'0' + rest as u8;
    }
    if value < 0 {
        at -= 1;
        buffer[at] = b'-';
    }
    // SAFETY: only ASCII was written.
    unsafe { std::str::from_utf8_unchecked(&buffer[at..]) }
}

crate::stubs::anchored! {
    /// The symbol of [`string_concat`].
    baml_rt_string_concat => string_concat
}

/// Join the `count` strings whose references are the words at `parts`.
///
/// # Safety
///
/// Called from compiled code with its mutator; `parts` is the address of
/// `count` string references in the caller's frame.
unsafe extern "C" fn string_concat(mutator: &mut Mutator, parts: usize, count: usize) -> usize {
    let (mut bytes, mut chars) = (0, 0);
    for index in 0..count {
        // SAFETY: the caller's contract.
        unsafe {
            let lengths = load(load(parts + index * WORD) + STRING_LENGTHS);
            bytes += lengths & 0xFFFF_FFFF;
            chars += lengths >> STRING_CHAR_COUNT_SHIFT;
        }
        // The allocation below can move the parts.
        mutator.root_at(parts + index * WORD);
    }
    let joined = allocate(mutator, bytes, chars);
    mutator.unroot(count);
    let mut cursor = joined + STRING_TEXT;
    for index in 0..count {
        // SAFETY: the parts were kept current, and `joined` has room for
        // all of their text.
        unsafe {
            let part = text(load(parts + index * WORD));
            std::ptr::copy_nonoverlapping(part.as_ptr(), cursor as *mut u8, part.len());
            cursor += part.len();
        }
    }
    joined
}

/// Whether two strings have the same text.
///
/// # Safety
///
/// Both are live string objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_eq(left: usize, right: usize) -> u8 {
    // SAFETY: the caller's contract.
    // Strings whose lengths differ are told apart without finding the text
    // of either.
    u8::from(
        left == right
            || unsafe {
                load(left + STRING_LENGTHS) == load(right + STRING_LENGTHS)
                    && text(left) == text(right)
            },
    )
}

/// Order two strings by their bytes: negative, zero or positive.
///
/// # Safety
///
/// Both are live string objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_cmp(left: usize, right: usize) -> i64 {
    // SAFETY: the caller's contract.
    unsafe { text(left).cmp(text(right)) as i64 }
}

crate::stubs::anchored! {
    /// The symbol of [`string_from_int`].
    baml_rt_string_from_int => string_from_int
}

/// The decimal text of an `int`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
unsafe extern "C" fn string_from_int(mutator: &mut Mutator, value: i64) -> usize {
    from_ascii(mutator, int_digits(value, &mut [0; INT_DIGITS]))
}

/// The text of a `float` inside a string: as a result prints, except that
/// the infinities are spelled out.
pub fn float_text(value: f64) -> String {
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    crate::float_text(value)
}

crate::stubs::anchored! {
    /// The symbol of [`string_from_float`].
    baml_rt_string_from_float => string_from_float
}

/// The text of a `float`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
unsafe extern "C" fn string_from_float(mutator: &mut Mutator, value: f64) -> usize {
    from_str(mutator, &float_text(value))
}

crate::stubs::anchored! {
    /// The symbol of [`string_from_any`].
    baml_rt_string_from_any => string_from_any
}

/// The text of the value of tag `tag` and payload `payload`, whose type
/// declares no `to_string` of its own: a string is itself, and an `int`, a
/// `float`, a `bool` or `null` is its text. An enum's variant is its name,
/// the string its object holds.
///
/// # Safety
///
/// Called from compiled code with its mutator and a value: when the tag
/// is a pointer's, the payload is a live object.
unsafe extern "C" fn string_from_any(mutator: &mut Mutator, tag: usize, payload: usize) -> usize {
    let value = Value { tag, payload };
    match value.id() {
        TAG_NULL => from_str(mutator, "null"),
        TAG_INT => from_ascii(
            mutator,
            int_digits(payload.cast_signed() as i64, &mut [0; INT_DIGITS]),
        ),
        TAG_FLOAT => from_str(mutator, &float_text(f64::from_bits(payload as u64))),
        TAG_BOOL => from_str(mutator, if payload & 0xFF != 0 { "true" } else { "false" }),
        string if string < STRING_TAGS_END && value.is_pointer() => payload,
        // SAFETY: the tag says it is one.
        crate::layout::TAG_BIGINT => {
            let text = unsafe { crate::bigint::value(payload) }.to_string();
            from_str(mutator, &text)
        }
        // A `uint8array` has a `to_string` of its own, which is what a
        // union that holds one is written by: its bytes as UTF-8, each
        // sequence that is not replaced.
        crate::layout::TAG_BYTES => {
            // SAFETY: the tag says it is one.
            let bytes = unsafe { crate::native::BytesRef::from_raw(payload) }.to_vec();
            from_str(mutator, &String::from_utf8_lossy(&bytes))
        }
        // SAFETY: the tag says it is a variant, which holds its name.
        crate::layout::TAG_ENUM_VARIANT => unsafe { load(payload + crate::layout::VARIANT_NAME) },
        // Anything else is written as an uncaught throw of it is.
        _ => {
            // SAFETY: the caller's contract.
            let text = unsafe {
                crate::class::context_text(mutator, payload).unwrap_or_else(|| {
                    let mut text = String::new();
                    crate::class::write_value(mutator, &mut text, value, None);
                    text
                })
            };
            from_str(mutator, &text)
        }
    }
}
