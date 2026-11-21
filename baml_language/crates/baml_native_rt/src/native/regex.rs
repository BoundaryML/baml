//! `baml.regex`. A `Regex` owns its compiled program: Rust state the heap
//! drops when the `Regex` is dead.
//!
//! The engines and the classification of a bad pattern are `sys_regex`'s,
//! which the compiler checks a constant pattern with too. It reports byte
//! offsets, and a BAML string is indexed by character.

use std::sync::Arc;

use sys_regex::{ErrorKind, Program, RawMatch, SearchAborted};

use super::{
    Arr, Class, Map, NativeBamlRegex, NativeBamlRegexRegex, Natives, Obj, Opaque, Root, Scope, Str,
    StrRef, Thrown, array_size, part_size,
};
use crate::layout::{NAMED_REGEX, NAMED_REGEX_ERROR, NAMED_REGEX_MATCH};

/// The fields of `baml.regex.Match` and `baml.regex.Group`, by position.
const MATCH_GROUPS: usize = 3;
const MATCH_NAMED: usize = 4;

fn offset(chars: usize) -> i64 {
    i64::try_from(chars).unwrap_or(i64::MAX)
}

/// The program of `regex`, held apart from the heap object that owns it.
fn program(scope: &Scope<'_>, regex: Root<'_, Obj>) -> Arc<Program> {
    // SAFETY: a `Regex` has one field, its program.
    unsafe { regex.get(scope).field::<Opaque>(0) }.share()
}

/// A search that ran out of budget is a panic, not "no match". Only a
/// pattern compiled with `backtracking = true` gets here.
fn aborted<'s>(scope: &mut Scope<'s>, program: &Program, abort: SearchAborted) -> Thrown<'s> {
    scope.user_panic(&format!(
        "baml.regex: {} while matching /{}/ — this pattern was compiled with `backtracking = \
         true`, whose matching time is not bounded",
        abort.reason,
        program.pattern()
    ))
}

/// Bytes `from..to` of `haystack`, which are `chars` characters: a slice of
/// it when long, as any other part of a string is.
fn part<'s>(
    scope: &mut Scope<'s>,
    haystack: Root<'s, Str>,
    (from, to): (usize, usize),
    chars: usize,
) -> Root<'s, Str> {
    let reservation = scope.reserve(part_size(to - from));
    let whole = reservation.get(haystack).whole();
    // SAFETY: a span an engine reports is inside the text and between
    // characters, and `chars` was counted over it.
    let text = unsafe { reservation.part(whole, from, to - from, chars) }.keep();
    reservation.finish(text)
}

/// A `Group` or the first three fields of a `Match`, which are the same:
/// the text of `span`, and where it starts and ends in characters.
fn spanned<'s>(
    scope: &mut Scope<'s>,
    class: Class,
    haystack: Root<'s, Str>,
    span: (usize, usize),
    chars: (usize, usize),
) -> super::Build<'s> {
    let text = part(scope, haystack, span, chars.1 - chars.0);
    let mut built = scope.build(class);
    built.set(scope, 0, text);
    built.set(scope, 1, offset(chars.0));
    built.set(scope, 2, offset(chars.1));
    built
}

/// The `Match` of `raw` in `haystack`, whose spans are at the characters
/// `chars`: a start and an end for each group that took part, in order.
fn match_value<'s>(
    scope: &mut Scope<'s>,
    program: &Program,
    haystack: Root<'s, Str>,
    raw: &RawMatch,
    chars: &[usize],
) -> Root<'s, Obj> {
    let class = scope.named_class(NAMED_REGEX_MATCH);
    let group = class
        .field_class(MATCH_GROUPS)
        .expect("a match's groups are of a class");

    let whole = if raw.groups.first().is_some_and(Option::is_some) {
        (chars[0], chars[1])
    } else {
        (0, 0)
    };
    let mut found = spanned(scope, class, haystack, raw.span(), whole);
    let groups: Root<'s, Arr<Obj>> = scope.alloc_array(raw.groups.len());
    let named: Root<'s, Map> = scope.alloc_str_map(program.names().iter().flatten().count());
    let mut next = 0;
    for (index, span) in raw.groups.iter().enumerate() {
        let name = program.names().get(index).and_then(Option::as_deref);
        // The group, its text and its name are reached through the array
        // and the map once the round is over.
        scope.scoped(|scope, _| {
            let made = span.map(|span| {
                let built = spanned(scope, group, haystack, span, (chars[next], chars[next + 1]));
                next += 2;
                built.finish()
            });
            groups.set(scope, index, made);
            // Every name of the pattern is a key, whether or not its
            // group took part.
            if let Some(name) = name {
                let key = scope.alloc_str(name);
                scope.str_map_set(named, key, made);
            }
        });
    }
    found.set(scope, MATCH_GROUPS, groups);
    found.set(scope, MATCH_NAMED, named);
    found.finish()
}

/// The `Match` of each of `matches` in `haystack`. Their spans are turned
/// into characters in one pass over the text: counting again from the start
/// for each match would make finding every match quadratic.
fn match_values<'s>(
    scope: &mut Scope<'s>,
    program: &Program,
    haystack: Root<'s, Str>,
    matches: &[RawMatch],
) -> Root<'s, Arr<Obj>> {
    let spans: Vec<usize> = matches
        .iter()
        .flat_map(|raw| &raw.groups)
        .flatten()
        .flat_map(|&(start, end)| [start, end])
        .collect();
    let chars = sys_regex::char_offsets(haystack.get(scope).as_str(), &spans);
    let found: Root<'s, Arr<Obj>> = scope.alloc_array(matches.len());
    let mut rest = chars.as_slice();
    for (index, raw) in matches.iter().enumerate() {
        let (own, after) = rest.split_at(raw.groups.iter().flatten().count() * 2);
        rest = after;
        scope.scoped(|scope, _| {
            let made = match_value(scope, program, haystack, raw, own);
            found.set(scope, index, Some(made));
        });
    }
    found
}

/// `haystack` with its first `limit` matches replaced by `template`, or
/// every match when `limit` is zero. A haystack nothing matched in is
/// returned as it is.
fn replaced<'s>(
    scope: &mut Scope<'s>,
    regex: Root<'s, Obj>,
    haystack: Root<'s, Str>,
    template: Root<'s, Str>,
    limit: usize,
) -> Result<Root<'s, Str>, Thrown<'s>> {
    let program = program(scope, regex);
    let replaced = program
        .replacen(
            haystack.get(scope).as_str(),
            limit,
            template.get(scope).as_str(),
        )
        .map(|text| match text {
            std::borrow::Cow::Borrowed(_) => None,
            std::borrow::Cow::Owned(text) => Some(text),
        });
    match replaced {
        Ok(None) => Ok(haystack),
        Ok(Some(text)) => Ok(scope.alloc_str(&text)),
        Err(abort) => Err(aborted(scope, &program, abort)),
    }
}

/// The `Match` a search for one found, when it found one.
fn first_match<'s>(
    scope: &mut Scope<'s>,
    program: &Program,
    haystack: Root<'s, Str>,
    found: Result<Option<RawMatch>, SearchAborted>,
) -> Result<Option<Root<'s, Obj>>, Thrown<'s>> {
    match found {
        Ok(Some(raw)) => {
            let spans: Vec<usize> = raw
                .groups
                .iter()
                .flatten()
                .flat_map(|&(start, end)| [start, end])
                .collect();
            let chars = sys_regex::char_offsets(haystack.get(scope).as_str(), &spans);
            Ok(Some(match_value(scope, program, haystack, &raw, &chars)))
        }
        Ok(None) => Ok(None),
        Err(abort) => Err(aborted(scope, program, abort)),
    }
}

impl NativeBamlRegex for Natives {
    fn _compile<'s>(
        scope: &mut Scope<'s>,
        pattern: Root<'s, Str>,
        backtracking: bool,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let error = match Program::compile(pattern.get(scope).as_str(), backtracking) {
            Ok(program) => {
                let program = scope.alloc_owned(Arc::new(program));
                let class = scope.named_class(NAMED_REGEX);
                let mut regex = scope.build(class);
                regex.set(scope, 0, program);
                return Ok(regex.finish());
            }
            Err(error) => error,
        };
        // The engine reports where in the pattern, in bytes.
        let span = error.span.map(|(start, end)| {
            let chars = sys_regex::char_offsets(pattern.get(scope).as_str(), &[start, end]);
            (offset(chars[0]), offset(chars[1]))
        });
        let message = scope.alloc_str(&error.message);
        let class = scope.named_class(NAMED_REGEX_ERROR);
        let mut thrown = scope.build(class);
        // The variants of `baml.regex.ErrorKind`, in declaration order.
        let kind = match error.kind {
            ErrorKind::Syntax => 0,
            ErrorKind::Unsupported => 1,
            ErrorKind::TooLarge => 2,
        };
        thrown.set(scope, 0, super::Value::Variant(kind));
        thrown.set(scope, 1, message);
        thrown.set(scope, 2, pattern);
        thrown.set(scope, 3, span.map(|(start, _)| start));
        thrown.set(scope, 4, span.map(|(_, end)| end));
        let thrown = thrown.finish();
        Err(scope.throw(thrown))
    }

    fn escape<'s>(scope: &mut Scope<'s>, literal: Root<'s, Str>) -> Root<'s, Str> {
        let escaped = sys_regex::escape(literal.get(scope).as_str());
        if escaped.len() == literal.get(scope).byte_len() {
            // Nothing in it needed a backslash.
            return literal;
        }
        scope.alloc_str(&escaped)
    }

    /// Where `search` first occurs in `haystack` at or after character
    /// `start`, in characters.
    fn _index_of_from(haystack: StrRef<'_>, search: StrRef<'_>, start: i64) -> Option<i64> {
        let start = usize::try_from(start)
            .ok()
            .filter(|start| *start <= haystack.char_count())?;
        let text = haystack.as_str();
        let from = if haystack.is_ascii() {
            start
        } else {
            text.char_indices()
                .nth(start)
                .map_or(text.len(), |(byte, _)| byte)
        };
        let found = from + text[from..].find(search.as_str())?;
        let chars = if haystack.is_ascii() {
            found
        } else {
            start + text[from..found].chars().count()
        };
        Some(offset(chars))
    }
}

impl NativeBamlRegexRegex for Natives {
    fn has_match<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
    ) -> Result<bool, Thrown<'s>> {
        // SAFETY: a `Regex` has one field, its program.
        let program = unsafe { regex.get(scope).field::<Opaque>(0) }.owned::<Program>();
        match program.is_match(haystack.get(scope).as_str()) {
            Ok(found) => Ok(found),
            Err(abort) => {
                let program = self::program(scope, regex);
                Err(aborted(scope, &program, abort))
            }
        }
    }

    fn match_once<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
    ) -> Result<Option<Root<'s, Obj>>, Thrown<'s>> {
        let program = program(scope, regex);
        let found = program.find_first(haystack.get(scope).as_str());
        first_match(scope, &program, haystack, found)
    }

    fn match_full<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
    ) -> Result<Option<Root<'s, Obj>>, Thrown<'s>> {
        let program = program(scope, regex);
        let found = program.find_exact(haystack.get(scope).as_str());
        first_match(scope, &program, haystack, found)
    }

    fn match_all<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
    ) -> Result<Root<'s, Arr<Obj>>, Thrown<'s>> {
        let program = program(scope, regex);
        match program.find_all(haystack.get(scope).as_str()) {
            Ok(found) => Ok(match_values(scope, &program, haystack, &found)),
            Err(abort) => Err(aborted(scope, &program, abort)),
        }
    }

    /// The fields between the matches, with what each group of a match
    /// took between them: every one a part of the haystack.
    fn split<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
    ) -> Result<Root<'s, Arr<Str>>, Thrown<'s>> {
        let program = program(scope, regex);
        let pieces = match program.split(haystack.get(scope).as_str()) {
            Ok(pieces) => pieces,
            Err(abort) => return Err(aborted(scope, &program, abort)),
        };
        let bytes: usize = pieces
            .iter()
            .map(|(start, end)| part_size(end - start))
            .sum();
        let reservation = scope.reserve(array_size(pieces.len()) + bytes);
        let array = reservation.array::<Str>(pieces.len());
        let whole = reservation.get(haystack).whole();
        for (index, &(start, end)) in pieces.iter().enumerate() {
            let chars = whole.as_str()[start..end].chars().count();
            // SAFETY: a span an engine reports is inside the text and
            // between characters, which were counted over it.
            array.set(index, unsafe {
                reservation.part(whole, start, end - start, chars)
            });
        }
        let array = array.keep();
        Ok(reservation.finish(array))
    }

    fn _replace_template<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
        template: Root<'s, Str>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        replaced(scope, regex, haystack, template, 1)
    }

    fn _replace_all_template<'s>(
        scope: &mut Scope<'s>,
        regex: Root<'s, Obj>,
        haystack: Root<'s, Str>,
        template: Root<'s, Str>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        replaced(scope, regex, haystack, template, 0)
    }
}
