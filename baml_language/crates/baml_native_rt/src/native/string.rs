//! `baml.String`.

use super::{
    Arr, Int, NativeBamlString, Natives, Root, Scope, Str, StrRef, Thrown, array_size, str_size,
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

/// Bytes `from..to` of `string` as a string: the string itself when that
/// is all of it, and otherwise a copy. `chars` is how many characters the
/// range has, when the caller knows.
fn substring<'s>(
    scope: &mut Scope<'s>,
    string: Root<'s, Str>,
    from: usize,
    to: usize,
    chars: Option<usize>,
) -> Root<'s, Str> {
    let view = string.get(scope);
    if from == 0 && to == view.as_str().len() {
        return string;
    }
    let ascii = view.is_ascii();
    let reservation = scope.reserve(str_size(to - from));
    let part = &reservation.get(string).as_str()[from..to];
    let chars = match chars {
        Some(chars) => chars,
        None if ascii => part.len(),
        None => part.chars().count(),
    };
    let mut copy = reservation.new_str(part.len(), chars);
    copy.push(part);
    let copy = copy.finish().keep();
    reservation.finish(copy)
}

/// A way to cut a string into parts.
trait Parts {
    fn of<'a>(text: &'a str, delimiter: &'a str) -> impl Iterator<Item = &'a str>;
}

/// Every character.
struct Characters;
/// Every line, without its ending.
struct Lines;
/// What lies between the occurrences of a delimiter that is not empty.
struct Between;

impl Parts for Characters {
    fn of<'a>(text: &'a str, _: &'a str) -> impl Iterator<Item = &'a str> {
        text.char_indices()
            .map(|(start, character)| &text[start..start + character.len_utf8()])
    }
}

impl Parts for Lines {
    fn of<'a>(text: &'a str, _: &'a str) -> impl Iterator<Item = &'a str> {
        text.lines()
    }
}

impl Parts for Between {
    fn of<'a>(text: &'a str, delimiter: &'a str) -> impl Iterator<Item = &'a str> {
        text.split(delimiter)
    }
}

/// An array of the parts of `string`, each a copy.
fn pieces<'s, P: Parts>(
    scope: &mut Scope<'s>,
    string: Root<'s, Str>,
    delimiter: Option<Root<'s, Str>>,
) -> Root<'s, Arr<Str>> {
    // Measure, reserve once, then build: nothing moves in between, so the
    // parts are copied straight out of the string.
    let view = string.get(scope);
    let ascii = view.is_ascii();
    let by = delimiter.map_or("", |delimiter| delimiter.get(scope).as_str());
    let (count, bytes) = P::of(view.as_str(), by).fold((0, 0), |(count, bytes), part| {
        (count + 1, bytes + str_size(part.len()))
    });
    let reservation = scope.reserve(array_size(count) + bytes);
    let array = reservation.array::<Str>(count);
    let by = delimiter.map_or("", |delimiter| reservation.get(delimiter).as_str());
    for (index, part) in P::of(reservation.get(string).as_str(), by).enumerate() {
        let chars = if ascii {
            part.len()
        } else {
            part.chars().count()
        };
        let mut copy = reservation.new_str(part.len(), chars);
        copy.push(part);
        array.set(index, copy.finish());
    }
    let array = array.keep();
    reservation.finish(array)
}

/// The size of a string of `bytes` bytes of text, which is refused here,
/// before room for it is asked for, when it is longer than a string can be.
fn text_size(bytes: usize) -> usize {
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
    let count = if from.as_str().is_empty() {
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
    let bytes = (text.as_str().len() - count * from.as_str().len())
        .saturating_add(count.saturating_mul(to.as_str().len()));
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
        int(string.as_str().len())
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
        let (length, chars) = (view.as_str().len(), view.char_count());
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
        pieces::<Characters>(scope, string, None)
    }

    fn lines<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Arr<Str>> {
        pieces::<Lines>(scope, string, None)
    }

    fn trim_start<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        let text = string.get(scope).as_str();
        let (from, to) = (text.len() - text.trim_start().len(), text.len());
        substring(scope, string, from, to, None)
    }

    fn trim_end<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        let to = string.get(scope).as_str().trim_end().len();
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
        if delimiter.get(scope).as_str().is_empty() {
            pieces::<Characters>(scope, string, None)
        } else {
            pieces::<Between>(scope, string, Some(delimiter))
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
        let from = text.len() - text.trim_start().len();
        let to = text.trim_end().len().max(from);
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
