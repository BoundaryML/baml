//! `baml.String`.

use super::{
    Arr, Int, NativeBamlString, Natives, Root, Scope, Str, StrRef, Thrown, array_size, part_size,
    str_size,
};

/// The position a `slice` bound names in a sequence of `length`: counted
/// back from the end when negative, then clamped into `[0, length]`.
fn slice_bound(bound: i64, length: usize) -> usize {
    let length = i64::try_from(length).unwrap_or(i64::MAX);
    let resolved = if bound < 0 { bound + length } else { bound };
    usize::try_from(resolved.clamp(0, length)).unwrap_or_default()
}

/// The byte at which character `index` of `text` starts, or the text's
/// length when it has no such character.
fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// The position an index names in a sequence of `length`: counted back from
/// the end when negative, and `None` outside `[0, length)`.
fn index_in(index: i64, length: usize) -> Option<usize> {
    let resolved = if index < 0 {
        index.checked_add(i64::try_from(length).ok()?)?
    } else {
        index
    };
    usize::try_from(resolved)
        .ok()
        .filter(|&resolved| resolved < length)
}

/// The character with the code point `value`, when there is one.
fn code_point(value: i64) -> Option<char> {
    u32::try_from(value).ok().and_then(char::from_u32)
}

fn int(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// The index of the character that starts at byte `byte` of `string`.
fn byte_to_char(string: StrRef<'_>, byte: usize) -> i64 {
    let text = string.as_str();
    int(if string.is_ascii() {
        byte
    } else if byte <= text.len() / 2 {
        text[..byte].chars().count()
    } else {
        // Count the shorter side.
        string.char_count() - text[byte..].chars().count()
    })
}

/// `search` as the one ASCII character it is, which is found a word at a
/// time instead of by string search.
fn single_ascii(search: &str) -> Option<char> {
    match search.as_bytes() {
        [byte] if byte.is_ascii() => Some(char::from(*byte)),
        _ => None,
    }
}

/// Whether every character of `string` passes: `ascii` is asked of each
/// byte when every character is one byte, and `unicode` of each character
/// otherwise. True of the empty string.
fn all(string: StrRef<'_>, ascii: impl Fn(&u8) -> bool, unicode: impl Fn(char) -> bool) -> bool {
    if string.is_ascii() {
        string.as_str().as_bytes().iter().all(ascii)
    } else {
        string.as_str().chars().all(unicode)
    }
}

/// Whether every byte of `string` passes, for a test no byte of a
/// character past ASCII does.
fn all_bytes(string: StrRef<'_>, test: impl Fn(&u8) -> bool) -> bool {
    string.as_str().as_bytes().iter().all(test)
}

/// Whether `byte` is one of the white space characters that are one byte.
fn is_space(byte: u8) -> bool {
    byte == b' ' || (b'\t'..=b'\r').contains(&byte)
}

/// How many bytes of white space `text` starts with.
fn leading_space(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut from = 0;
    while from < bytes.len() && is_space(bytes[from]) {
        from += 1;
    }
    if bytes.get(from).is_some_and(|byte| !byte.is_ascii()) {
        // White space past ASCII is left to the tables.
        from = text.len() - text[from..].trim_start().len();
    }
    from
}

/// How many bytes of `text` come before the white space it ends with.
fn before_trailing_space(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut to = bytes.len();
    while to > 0 && is_space(bytes[to - 1]) {
        to -= 1;
    }
    if to > 0 && !bytes[to - 1].is_ascii() {
        to = text[..to].trim_end().len();
    }
    to
}

/// Bytes `from..to` of `string` as a string: the string itself when that
/// is all of it, and otherwise a copy when short and a slice when not.
/// `chars` is how many characters the range has, when the caller knows.
fn substring<'s>(
    scope: &mut Scope<'s>,
    string: Root<'s, Str>,
    from: usize,
    to: usize,
    chars: Option<usize>,
) -> Root<'s, Str> {
    let view = string.get(scope);
    let text = view.as_str();
    if from == 0 && to == text.len() {
        return string;
    }
    let chars = match chars {
        Some(chars) => chars,
        None if view.is_ascii() => to - from,
        // Counted in what is left out, which is the shorter for a trim.
        None => view.char_count() - text[..from].chars().count() - text[to..].chars().count(),
    };
    let reservation = scope.reserve(part_size(to - from));
    // SAFETY: the callers pass a range between characters, and its count
    // of them when they pass one.
    let whole = reservation.get(string).whole();
    let part = unsafe { reservation.part(whole, from, to - from, chars) }.keep();
    reservation.finish(part)
}

/// A way to cut a string into parts.
trait Parts {
    /// Whether a part is worth a slice: a character is not.
    const SLICES: bool;
    fn of(text: &str) -> impl Iterator<Item = &str>;
}

/// Every character.
struct Characters;
/// Every line, without its ending.
struct Lines;

impl Parts for Characters {
    const SLICES: bool = false;
    fn of(text: &str) -> impl Iterator<Item = &str> {
        text.char_indices()
            .map(|(start, character)| &text[start..start + character.len_utf8()])
    }
}

impl Parts for Lines {
    const SLICES: bool = true;
    fn of(text: &str) -> impl Iterator<Item = &str> {
        text.lines()
    }
}

/// An array of the parts of `string`.
fn pieces<'s, P: Parts>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Arr<Str>> {
    // Measure, reserve once, then build: nothing moves in between, so the
    // parts are taken straight out of the string.
    let view = string.get(scope);
    let ascii = view.is_ascii();
    let (count, bytes) = P::of(view.as_str()).fold((0, 0), |(count, bytes), part| {
        let size = if P::SLICES {
            part_size(part.len())
        } else {
            str_size(part.len())
        };
        (count + 1, bytes + size)
    });
    let reservation = scope.reserve(array_size(count) + bytes);
    let array = reservation.array::<Str>(count);
    let whole = reservation.get(string).whole();
    let text = whole.as_str();
    for (index, part) in P::of(text).enumerate() {
        let chars = if ascii {
            part.len()
        } else {
            part.chars().count()
        };
        array.set(
            index,
            if P::SLICES {
                let from = part.as_ptr() as usize - text.as_ptr() as usize;
                // SAFETY: `part` is that range of `text`.
                unsafe { reservation.part(whole, from, part.len(), chars) }
            } else {
                reservation.counted_str(part, chars)
            },
        );
    }
    let array = array.keep();
    reservation.finish(array)
}

/// How many bits of an [`equal_bytes`] mask stand for one byte.
#[cfg(target_arch = "aarch64")]
const BITS_PER_BYTE: u32 = 4;
#[cfg(target_arch = "x86_64")]
const BITS_PER_BYTE: u32 = 1;

/// Which of the sixteen bytes at `block` are `byte`: the lowest of the
/// [`BITS_PER_BYTE`] bits of byte `i` is set when it is.
///
/// # Safety
///
/// Sixteen bytes can be read at `block`.
#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn equal_bytes(block: *const u8, byte: u8) -> u64 {
    use std::arch::aarch64::{
        vceqq_u8, vdupq_n_u8, vget_lane_u64, vld1q_u8, vreinterpret_u64_u8, vreinterpretq_u16_u8,
        vshrn_n_u16,
    };
    // SAFETY: the caller's contract, and every aarch64 has these
    // instructions.
    unsafe {
        let equal = vceqq_u8(vld1q_u8(block), vdupq_n_u8(byte));
        // Each byte's eight bits narrowed to four.
        let narrowed = vshrn_n_u16::<4>(vreinterpretq_u16_u8(equal));
        vget_lane_u64::<0>(vreinterpret_u64_u8(narrowed)) & 0x1111_1111_1111_1111
    }
}

/// Which of the sixteen bytes at `block` are `byte`: bit `i` is set when
/// byte `i` is.
///
/// # Safety
///
/// Sixteen bytes can be read at `block`.
#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn equal_bytes(block: *const u8, byte: u8) -> u64 {
    use std::arch::x86_64::{
        __m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8,
    };
    // SAFETY: the caller's contract, and every x86-64 has these
    // instructions.
    let equal = unsafe {
        let block = _mm_loadu_si128(block.cast::<__m128i>());
        _mm_movemask_epi8(_mm_cmpeq_epi8(block, _mm_set1_epi8(byte.cast_signed())))
    };
    u64::from(equal.cast_unsigned())
}

/// Call `found` with the position of every byte of `text` that is `byte`,
/// front to back, sixteen bytes at a time where the machine can.
#[inline]
fn each_byte(text: &[u8], byte: u8, mut found: impl FnMut(usize)) {
    #[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
    if text.len() >= 16 {
        let mut at = 0;
        while at + 16 <= text.len() {
            // SAFETY: `at + 16` is inside `text`.
            let mut equal = unsafe { equal_bytes(text.as_ptr().add(at), byte) };
            while equal != 0 {
                found(at + (equal.trailing_zeros() / BITS_PER_BYTE) as usize);
                equal &= equal - 1;
            }
            at += 16;
        }
        if at < text.len() {
            // The last sixteen bytes, less the ones already looked at.
            let last = text.len() - 16;
            let seen = (at - last) as u32 * BITS_PER_BYTE;
            // SAFETY: `last + 16` is the end of `text`.
            let mut equal = unsafe { equal_bytes(text.as_ptr().add(last), byte) } >> seen << seen;
            while equal != 0 {
                found(last + (equal.trailing_zeros() / BITS_PER_BYTE) as usize);
                equal &= equal - 1;
            }
        }
        return;
    }
    for (at, &other) in text.iter().enumerate() {
        if other == byte {
            found(at);
        }
    }
}

/// Call `found` with the position of every occurrence of `delimiter`, which
/// is not empty, in `text`: front to back, none overlapping the one before.
#[inline]
fn each_occurrence(text: &[u8], delimiter: &[u8], mut found: impl FnMut(usize)) {
    let (first, rest) = (delimiter[0], &delimiter[1..]);
    let mut free = 0;
    each_byte(text, first, |at| {
        if at >= free && (rest.is_empty() || text[at + 1..].starts_with(rest)) {
            found(at);
            free = at + delimiter.len();
        }
    });
}

/// An array of what lies between the occurrences of `delimiter`, which is
/// not empty, in `string`.
fn between<'s>(
    scope: &mut Scope<'s>,
    string: Root<'s, Str>,
    delimiter: Root<'s, Str>,
) -> Root<'s, Arr<Str>> {
    // Measure, reserve once, then build: nothing moves in between, so the
    // parts are taken straight out of the string.
    let view = string.get(scope);
    let ascii = view.is_ascii();
    let (text, by) = (view.as_str(), delimiter.get(scope).as_str());
    let (mut count, mut bytes, mut from) = (1, 0, 0);
    each_occurrence(text.as_bytes(), by.as_bytes(), |at| {
        count += 1;
        bytes += part_size(at - from);
        from = at + by.len();
    });
    bytes += part_size(text.len() - from);
    let reservation = scope.reserve(array_size(count) + bytes);
    let array = reservation.array::<Str>(count);
    let whole = reservation.get(string).whole();
    let (text, by) = (whole.as_str(), reservation.get(delimiter).as_str());
    let (mut index, mut from) = (0, 0);
    let mut part = |from: usize, to: usize| {
        // SAFETY: an occurrence of a string in another starts and ends
        // between characters, and so does what lies between two.
        let part = unsafe {
            let chars = if ascii {
                to - from
            } else {
                text.get_unchecked(from..to).chars().count()
            };
            reservation.part(whole, from, to - from, chars)
        };
        array.set(index, part);
        index += 1;
    };
    each_occurrence(text.as_bytes(), by.as_bytes(), |at| {
        part(from, at);
        from = at + by.len();
    });
    part(from, text.len());
    let array = array.keep();
    reservation.finish(array)
}

/// The size of a string of `bytes` bytes of text, which is refused here,
/// before room for it is asked for, when it is longer than a string can be.
pub(super) fn text_size(bytes: usize) -> usize {
    assert!(
        u32::try_from(bytes).is_ok(),
        "a string cannot be longer than 4 GiB"
    );
    str_size(bytes)
}

/// `string` in the other case.
fn recase<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>, upper: bool) -> Root<'s, Str> {
    let view = string.get(scope);
    let text = view.as_str();
    if !view.is_ascii() {
        // Past ASCII a character's case can depend on its neighbours and
        // change its length.
        let recased = if upper {
            text.to_uppercase()
        } else {
            text.to_lowercase()
        };
        return scope.alloc_str(&recased);
    }
    let changes = if upper {
        text.bytes().any(|byte| byte.is_ascii_lowercase())
    } else {
        text.bytes().any(|byte| byte.is_ascii_uppercase())
    };
    if !changes {
        return string;
    }
    let length = text.len();
    let reservation = scope.reserve(str_size(length));
    let mut recased = reservation.new_str(length, length);
    recased.push(reservation.get(string).as_str());
    if upper {
        recased.make_ascii_uppercase();
    } else {
        recased.make_ascii_lowercase();
    }
    let recased = recased.finish().keep();
    reservation.finish(recased)
}

/// `string` with its occurrences of `search` replaced, the first only or
/// all of them. An empty `search` occurs before every character and at the
/// end.
fn replace<'s>(
    scope: &mut Scope<'s>,
    string: Root<'s, Str>,
    search: Root<'s, Str>,
    replacement: Root<'s, Str>,
    limit: usize,
) -> Root<'s, Str> {
    let (text, from, to) = (string.get(scope), search.get(scope), replacement.get(scope));
    let count = if from.byte_len() == 0 {
        text.char_count() + 1
    } else if limit == 1 {
        usize::from(text.as_str().contains(from.as_str()))
    } else {
        text.as_str().matches(from.as_str()).count()
    };
    let count = count.min(limit);
    if count == 0 {
        return string;
    }
    // Each occurrence is `search` out and `replacement` in.
    let bytes = (text.byte_len() - count * from.byte_len())
        .saturating_add(count.saturating_mul(to.byte_len()));
    let chars = (text.char_count() - count * from.char_count())
        .saturating_add(count.saturating_mul(to.char_count()));
    let reservation = scope.reserve(text_size(bytes));
    let (text, from, to) = (
        reservation.get(string).as_str(),
        reservation.get(search).as_str(),
        reservation.get(replacement).as_str(),
    );
    let mut replaced = reservation.new_str(bytes, chars);
    let mut kept = 0;
    for (start, found) in text.match_indices(from).take(count) {
        replaced.push(&text[kept..start]);
        replaced.push(to);
        kept = start + found.len();
    }
    replaced.push(&text[kept..]);
    let replaced = replaced.finish().keep();
    reservation.finish(replaced)
}

impl NativeBamlString for Natives {
    fn byte_length(string: StrRef<'_>) -> i64 {
        int(string.byte_len())
    }

    fn to_lower_case<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        recase(scope, string, false)
    }

    fn to_upper_case<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        recase(scope, string, true)
    }

    fn includes(string: StrRef<'_>, search: StrRef<'_>) -> bool {
        string.as_str().contains(search.as_str())
    }

    fn starts_with(string: StrRef<'_>, prefix: StrRef<'_>) -> bool {
        string
            .as_str()
            .as_bytes()
            .starts_with(prefix.as_str().as_bytes())
    }

    fn ends_with(string: StrRef<'_>, suffix: StrRef<'_>) -> bool {
        string
            .as_str()
            .as_bytes()
            .ends_with(suffix.as_str().as_bytes())
    }

    fn index_of(string: StrRef<'_>, search: StrRef<'_>) -> Option<i64> {
        let (text, search) = (string.as_str(), search.as_str());
        let byte = match single_ascii(search) {
            Some(character) => text.find(character),
            None => text.find(search),
        }?;
        Some(byte_to_char(string, byte))
    }

    fn last_index_of(string: StrRef<'_>, search: StrRef<'_>) -> Option<i64> {
        let (text, search) = (string.as_str(), search.as_str());
        let byte = match single_ascii(search) {
            Some(character) => text.rfind(character),
            None => text.rfind(search),
        }?;
        Some(byte_to_char(string, byte))
    }

    fn at<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>, index: i64) -> Option<Root<'s, Str>> {
        let view = string.get(scope);
        let index = index_in(index, view.char_count())?;
        let (from, to) = if view.is_ascii() {
            (index, index + 1)
        } else {
            let text = view.as_str();
            let from = char_to_byte(text, index);
            let width = text[from..].chars().next().map_or(0, char::len_utf8);
            (from, from + width)
        };
        Some(substring(scope, string, from, to, Some(1)))
    }

    fn code_point_at(string: StrRef<'_>, index: i64) -> Option<i64> {
        let index = index_in(index, string.char_count())?;
        let text = string.as_str();
        if string.is_ascii() {
            text.as_bytes().get(index).map(|&byte| i64::from(byte))
        } else {
            text.chars()
                .nth(index)
                .map(|character| i64::from(u32::from(character)))
        }
    }

    fn repeat<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>, count: i64) -> Root<'s, Str> {
        let count = usize::try_from(count).unwrap_or(0);
        let view = string.get(scope);
        let (length, chars) = (view.byte_len(), view.char_count());
        if count == 1 || (length == 0 && count != 0) {
            return string;
        }
        let bytes = length.saturating_mul(count);
        let reservation = scope.reserve(text_size(bytes));
        let mut repeated = reservation.new_str(bytes, chars.saturating_mul(count));
        let text = reservation.get(string).as_str();
        for _ in 0..count {
            repeated.push(text);
        }
        let repeated = repeated.finish().keep();
        reservation.finish(repeated)
    }

    fn _replace_literal<'s>(
        scope: &mut Scope<'s>,
        string: Root<'s, Str>,
        search: Root<'s, Str>,
        replacement: Root<'s, Str>,
    ) -> Root<'s, Str> {
        replace(scope, string, search, replacement, 1)
    }

    fn _replace_all_literal<'s>(
        scope: &mut Scope<'s>,
        string: Root<'s, Str>,
        search: Root<'s, Str>,
        replacement: Root<'s, Str>,
    ) -> Root<'s, Str> {
        replace(scope, string, search, replacement, usize::MAX)
    }

    fn chars<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Arr<Str>> {
        pieces::<Characters>(scope, string)
    }

    fn lines<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Arr<Str>> {
        pieces::<Lines>(scope, string)
    }

    fn trim_start<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        let text = string.get(scope).as_str();
        let (from, to) = (leading_space(text), text.len());
        substring(scope, string, from, to, None)
    }

    fn trim_end<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        let to = before_trailing_space(string.get(scope).as_str());
        substring(scope, string, 0, to, None)
    }

    fn is_numeric(string: StrRef<'_>) -> bool {
        all(string, u8::is_ascii_digit, char::is_numeric)
    }

    fn is_alphabetic(string: StrRef<'_>) -> bool {
        all(string, u8::is_ascii_alphabetic, char::is_alphabetic)
    }

    fn is_alphanumeric(string: StrRef<'_>) -> bool {
        all(string, u8::is_ascii_alphanumeric, char::is_alphanumeric)
    }

    fn is_uppercase(string: StrRef<'_>) -> bool {
        all(string, u8::is_ascii_uppercase, char::is_uppercase)
    }

    fn is_lowercase(string: StrRef<'_>) -> bool {
        all(string, u8::is_ascii_lowercase, char::is_lowercase)
    }

    fn is_whitespace(string: StrRef<'_>) -> bool {
        // Unlike `u8::is_ascii_whitespace`, a vertical tab is white space.
        all(
            string,
            |&byte| char::from(byte).is_whitespace(),
            char::is_whitespace,
        )
    }

    fn is_control(string: StrRef<'_>) -> bool {
        all(string, u8::is_ascii_control, char::is_control)
    }

    fn is_graphic(string: StrRef<'_>) -> bool {
        all(
            string,
            |&byte| !byte.is_ascii_control() && !char::from(byte).is_whitespace(),
            |character| !character.is_control() && !character.is_whitespace(),
        )
    }

    fn is_ascii(string: StrRef<'_>) -> bool {
        string.is_ascii()
    }

    fn is_ascii_numeric(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_digit)
    }

    fn is_ascii_alphabetic(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_alphabetic)
    }

    fn is_ascii_alphanumeric(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_alphanumeric)
    }

    fn is_ascii_uppercase(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_uppercase)
    }

    fn is_ascii_lowercase(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_lowercase)
    }

    fn is_ascii_whitespace(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_whitespace)
    }

    fn is_ascii_control(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_control)
    }

    fn is_ascii_graphic(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_graphic)
    }

    fn is_ascii_hex(string: StrRef<'_>) -> bool {
        all_bytes(string, u8::is_ascii_hexdigit)
    }

    fn from_code_points<'s>(
        scope: &mut Scope<'s>,
        unicode: Root<'s, Arr<Int>>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let view = unicode.get(scope);
        let mut bytes = 0;
        let mut invalid = None;
        for (index, &value) in view.as_slice().iter().enumerate() {
            match code_point(value) {
                Some(character) => bytes += character.len_utf8(),
                None => {
                    invalid = Some((index, value));
                    break;
                }
            }
        }
        if let Some((index, value)) = invalid {
            return Err(scope.invalid_argument(&format!(
                "string.from_code_points: value {value} at index {index} is not a valid Unicode code point (must be in [0, 0x10FFFF] and not a surrogate)"
            )));
        }
        let chars = view.len();
        let reservation = scope.reserve(text_size(bytes));
        let mut string = reservation.new_str(bytes, chars);
        for &value in reservation.get(unicode).as_slice() {
            if let Some(character) = code_point(value) {
                string.push_char(character);
            }
        }
        let string = string.finish().keep();
        Ok(reservation.finish(string))
    }

    fn split<'s>(
        scope: &mut Scope<'s>,
        string: Root<'s, Str>,
        delimiter: Root<'s, Str>,
    ) -> Root<'s, Arr<Str>> {
        if delimiter.get(scope).byte_len() == 0 {
            pieces::<Characters>(scope, string)
        } else {
            between(scope, string, delimiter)
        }
    }

    fn slice<'s>(
        scope: &mut Scope<'s>,
        string: Root<'s, Str>,
        start: i64,
        end: i64,
    ) -> Root<'s, Str> {
        let view = string.get(scope);
        let length = view.char_count();
        let start = slice_bound(start, length);
        let end = slice_bound(end, length).max(start);
        let text = view.as_str();
        let (from, to) = if view.is_ascii() {
            (start, end)
        } else {
            let from = char_to_byte(text, start);
            (from, from + char_to_byte(&text[from..], end - start))
        };
        substring(scope, string, from, to, Some(end - start))
    }

    fn trim<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        let text = string.get(scope).as_str();
        let from = leading_space(text);
        let to = before_trailing_space(text).max(from);
        substring(scope, string, from, to, None)
    }

    fn to_code_points<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Arr<Int>> {
        let count = string.get(scope).char_count();
        let reservation = scope.reserve(array_size(count));
        let array = reservation.array::<Int>(count);
        for (index, character) in reservation.get(string).as_str().chars().enumerate() {
            array.set(index, i64::from(u32::from(character)));
        }
        let array = array.keep();
        reservation.finish(array)
    }
}
