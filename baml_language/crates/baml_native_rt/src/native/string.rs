//! `baml.String`.

use super::{Arr, Int, NativeBamlString, Natives, Root, Scope, Str, StrRef, array_size, str_size};

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

/// The parts `split` gives: every character for an empty delimiter, and
/// otherwise what lies between the delimiter's occurrences.
fn parts<'a>(text: &'a str, delimiter: &'a str) -> Box<dyn Iterator<Item = &'a str> + 'a> {
    if delimiter.is_empty() {
        Box::new(
            text.char_indices()
                .map(|(start, character)| &text[start..start + character.len_utf8()]),
        )
    } else {
        Box::new(text.split(delimiter))
    }
}

impl NativeBamlString for Natives {
    fn includes(string: StrRef<'_>, search: StrRef<'_>) -> bool {
        string.as_str().contains(search.as_str())
    }

    fn split<'s>(
        scope: &mut Scope<'s>,
        string: Root<'s, Str>,
        delimiter: Root<'s, Str>,
    ) -> Root<'s, Arr<Str>> {
        // Measure, reserve once, then build: nothing moves in between, so
        // the parts are copied straight out of the string.
        let (count, bytes) = parts(string.get(scope).as_str(), delimiter.get(scope).as_str())
            .fold((0, 0), |(count, bytes), part| {
                (count + 1, bytes + str_size(part.len()))
            });
        let reservation = scope.reserve(array_size(count) + bytes);
        let array = reservation.array::<Str>(count);
        let text = reservation.get(string).as_str();
        for (index, part) in parts(text, reservation.get(delimiter).as_str()).enumerate() {
            array.set(index, reservation.str(part));
        }
        let array = array.keep();
        reservation.finish(array)
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
        if start == 0 && end == length {
            return string;
        }
        let text = view.as_str();
        let (from, to) = if view.is_ascii() {
            (start, end)
        } else {
            let from = char_to_byte(text, start);
            (from, from + char_to_byte(&text[from..], end - start))
        };
        let reservation = scope.reserve(str_size(to - from));
        let part = reservation.str(&reservation.get(string).as_str()[from..to]);
        let part = part.keep();
        reservation.finish(part)
    }

    fn trim<'s>(scope: &mut Scope<'s>, string: Root<'s, Str>) -> Root<'s, Str> {
        let text = string.get(scope).as_str();
        let trimmed = text.trim();
        if trimmed.len() == text.len() {
            return string;
        }
        let from = trimmed.as_ptr() as usize - text.as_ptr() as usize;
        let to = from + trimmed.len();
        let reservation = scope.reserve(str_size(to - from));
        let part = reservation.str(&reservation.get(string).as_str()[from..to]);
        let part = part.keep();
        reservation.finish(part)
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
