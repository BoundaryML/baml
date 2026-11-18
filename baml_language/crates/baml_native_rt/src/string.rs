//! Strings: immutable UTF-8 with a cached count of characters, so that
//! `length()` is a load.

use crate::{
    heap::{Mutator, current_frame, load, store},
    layout::{
        BOX_VALUE, SLICE_OFFSET, SLICE_PARENT, STRING_CHAR_COUNT_SHIFT, STRING_LENGTHS,
        STRING_TEXT, STRING_TYPES_END, TYPE_BOX_BOOL, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_ID_MASK,
        TYPE_STRING, WORD, words,
    },
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
        let start = if load(string) & TYPE_ID_MASK == TYPE_STRING {
            string + STRING_TEXT
        } else {
            load(string + SLICE_PARENT) + STRING_TEXT + load(string + SLICE_OFFSET)
        };
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(start as *const u8, length))
    }
}

/// Allocate a string of `bytes` bytes and `chars` characters with its text
/// left to be written. May collect.
fn allocate(mutator: &mut Mutator, bytes: usize, chars: usize) -> usize {
    assert!(
        u32::try_from(bytes).is_ok(),
        "a string cannot be longer than 4 GiB"
    );
    let string = mutator.allocate_object(TYPE_STRING, STRING_TEXT + words(bytes));
    // SAFETY: the object is at least `STRING_TEXT` bytes.
    unsafe {
        store(
            string + STRING_LENGTHS,
            bytes | (chars << STRING_CHAR_COUNT_SHIFT),
        );
    };
    string
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

/// Join the `count` strings whose references are the words at `parts`.
///
/// # Safety
///
/// Called from compiled code with its mutator; `parts` is the address of
/// `count` string references in the caller's frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_concat(
    mutator: &mut Mutator,
    parts: usize,
    count: usize,
) -> usize {
    mutator.enter(current_frame());
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

/// The decimal text of an `int`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_from_int(mutator: &mut Mutator, value: i64) -> usize {
    mutator.enter(current_frame());
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

/// The text of a `float`.
///
/// # Safety
///
/// Called from compiled code with its mutator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_from_float(mutator: &mut Mutator, value: f64) -> usize {
    mutator.enter(current_frame());
    from_str(mutator, &float_text(value))
}

/// The name of variant `index` of the enum whose boxes have the type
/// `enum_type`, as a string.
///
/// # Safety
///
/// Called from compiled code with its mutator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_from_enum(
    mutator: &mut Mutator,
    enum_type: usize,
    index: usize,
) -> usize {
    mutator.enter(current_frame());
    let name = mutator.variant_name(enum_type, index);
    from_str(mutator, name)
}

/// The text of a value held as a reference whose type declares no
/// `to_string` of its own: a string is itself, and a box is the text of
/// the `int`, `float`, `bool` or enum variant in it.
///
/// # Safety
///
/// Called from compiled code with its mutator; `value` is a live string or
/// box.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_string_from_any(mutator: &mut Mutator, value: usize) -> usize {
    mutator.enter(current_frame());
    // SAFETY: the caller's contract: a box is a header and one value.
    let (type_id, held) = unsafe { (load(value) & TYPE_ID_MASK, load(value + BOX_VALUE)) };
    match type_id {
        string if string < STRING_TYPES_END => value,
        TYPE_BOX_INT => from_ascii(
            mutator,
            int_digits(held.cast_signed() as i64, &mut [0; INT_DIGITS]),
        ),
        TYPE_BOX_FLOAT => from_str(mutator, &float_text(f64::from_bits(held as u64))),
        TYPE_BOX_BOOL => from_str(mutator, if held & 0xFF != 0 { "true" } else { "false" }),
        enum_type if !mutator.is_reference(enum_type) => {
            let name = mutator.variant_name(enum_type, held);
            from_str(mutator, name)
        }
        // Anything else is written as an uncaught throw of it is.
        _ => {
            // SAFETY: the caller's contract.
            let text = unsafe {
                crate::class::context_text(mutator, value).unwrap_or_else(|| {
                    let mut text = String::new();
                    crate::class::write_value(mutator, &mut text, value);
                    text
                })
            };
            from_str(mutator, &text)
        }
    }
}
