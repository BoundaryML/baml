use std::convert::TryFrom;

use minijinja::value::{Kwargs, Value, ValueKind};
use minijinja::State;
use minijinja::{Error, ErrorKind};

#[cfg(feature = "datetime")]
mod datetime;

#[cfg(feature = "datetime")]
pub use self::datetime::*;

#[cfg(feature = "html_entities")]
use crate::html_entities::HTML_ENTITIES;

// this list has to be ASCII sorted because we're going to binary search through it.
#[cfg(not(feature = "html_entities"))]
const HTML_ENTITIES: &[(&str, &str)] = &[("amp", "&"), ("gt", ">"), ("lt", "<"), ("quot", "\"")];

/// Returns a plural suffix if the value is not 1, '1', or an object of
/// length 1.
///
/// By default, the plural suffix is 's' and the singular suffix is
/// empty (''). You can specify a singular suffix as the first argument (or
/// `None`, for the default). You can specify a plural suffix as the second
/// argument (or `None`, for the default).
///
/// ```jinja
/// {{ users|length }} user{{ users|pluralize }}.
/// ```
///
/// ```jinja
/// {{ entities|length }} entit{{ entities|pluralize("y", "ies") }}.
/// ```
///
/// ```jinja
/// {{ platypuses|length }} platypus{{ platypuses|pluralize(None, "es") }}.
/// ```
pub fn pluralize(
    v: &Value,
    singular: Option<Value>,
    plural: Option<Value>,
) -> Result<Value, Error> {
    let is_singular = match v.len() {
        Some(val) => val == 1,
        None => match i64::try_from(v.clone()) {
            Ok(val) => val == 1,
            Err(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidOperation,
                    format!(
                        "Pluralize argument is not an integer, or a sequence / object with a \
                         length but of type {}",
                        v.kind()
                    ),
                ));
            }
        },
    };

    let (rv, default) = if is_singular {
        (singular.unwrap_or(Value::UNDEFINED), "")
    } else {
        (plural.unwrap_or(Value::UNDEFINED), "s")
    };

    if rv.is_undefined() || rv.is_none() {
        Ok(Value::from(default))
    } else {
        Ok(rv)
    }
}

/// Chooses a random element from a sequence or string.
///
/// The random number generated can be seeded with the `RAND_SEED`
/// global context variable.
///
/// ```jinja
/// {{ [1, 2, 3, 4]|random }}
/// ```
#[cfg(feature = "rand")]
#[cfg_attr(docsrs, doc(cfg(feature = "rand")))]
pub fn random(state: &minijinja::State, seq: &Value) -> Result<Value, Error> {
    use minijinja::value::ValueKind;

    if matches!(seq.kind(), ValueKind::Seq | ValueKind::String) {
        let len = seq.len().unwrap_or(0);
        let idx = crate::rand::XorShiftRng::for_state(state).next_usize(len);
        seq.get_item_by_index(idx)
    } else {
        Err(Error::new(
            ErrorKind::InvalidOperation,
            "can only select random elements from sequences",
        ))
    }
}

/// Formats the value like a "human-readable" file size.
///
/// For example. 13 kB, 4.1 MB, 102 Bytes, etc.  Per default decimal prefixes are
/// used (Mega, Giga, etc.),  if the second parameter is set to true
/// the binary prefixes are used (Mebi, Gibi).
pub fn filesizeformat(value: f64, binary: Option<bool>) -> String {
    const BIN_PREFIXES: &[&str] = &["KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
    const SI_PREFIXES: &[&str] = &["kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let (prefixes, base) = if binary.unwrap_or(false) {
        (BIN_PREFIXES, 1024.0)
    } else {
        (SI_PREFIXES, 1000.0)
    };

    if value == 1.0 {
        return "1 Byte".into();
    }
    let (sign, value) = if value < 0.0 {
        ("-", -value)
    } else {
        ("", value)
    };

    if value < base {
        format!("{sign}{value} Bytes")
    } else {
        for (idx, prefix) in prefixes.iter().enumerate() {
            let unit = base.powf(idx as f64 + 2.0);
            if value < unit || idx == prefixes.len() - 1 {
                return format!("{}{:.1} {}", sign, base * value / unit, prefix);
            }
        }
        unreachable!();
    }
}

/// Returns a truncated copy of the string.
///
/// The string will be truncated to the specified length, with an ellipsis
/// appended if truncation occurs. By default, the filter tries to preserve
/// whole words.
///
/// ```jinja
/// {{ "Hello World"|truncate(length=5) }}
/// ```
///
/// The filter accepts a few keyword arguments:
/// * `length`: maximum length of the output string (defaults to 255)
/// * `killwords`: set to `true` if you want to cut text exactly at length; if `false`,
///   the filter will preserve last word (defaults to `false`)
/// * `end`: if you want a specific ellipsis sign you can specify it (defaults to "...")
/// * `leeway`: determines the tolerance margin before truncation occurs (defaults to 5)
///
/// The truncation only occurs if the string length exceeds both the specified
/// length and the leeway margin combined. This means that if a string is just
/// slightly longer than the target length (within the leeway value), it will
/// be left unmodified.
///
/// When `killwords` is set to false (default behavior), the function ensures
/// that words remain intact by finding the last complete word that fits within
/// the length limit. This prevents words from being cut in the middle and
/// maintains text readability.
///
/// The specified length parameter is inclusive of the end string (ellipsis).
/// For example, with a length of 5 and the default ellipsis "...", only 2
/// characters from the original string will be preserved.
///
/// # Example with all attributes
/// ```jinja
/// {{ "Hello World"|truncate(
///     length=5,
///     killwords=true,
///     end='...',
///     leeway=2
/// ) }}
/// ```
pub fn truncate(state: &State, value: &Value, kwargs: Kwargs) -> Result<String, Error> {
    if matches!(value.kind(), ValueKind::None | ValueKind::Undefined) {
        return Ok("".into());
    }

    let s = value.as_str().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("expected string, got {}", value.kind()),
        )
    })?;

    let length = kwargs.get::<Option<usize>>("length")?.unwrap_or(255);
    let killwords = kwargs.get::<Option<bool>>("killwords")?.unwrap_or_default();
    let end = kwargs.get::<Option<&str>>("end")?.unwrap_or("...");
    let leeway = kwargs.get::<Option<usize>>("leeway")?.unwrap_or_else(|| {
        state
            .lookup("TRUNCATE_LEEWAY")
            .and_then(|x| usize::try_from(x.clone()).ok())
            .unwrap_or(5)
    });

    kwargs.assert_all_used()?;

    let end_len = end.chars().count();
    if length < end_len {
        return Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("expected length >= {end_len}, got {length}"),
        ));
    }

    if s.chars().count() <= length + leeway {
        return Ok(s.to_string());
    }

    let trunc_pos = length - end_len;
    let truncated = if killwords {
        s.chars().take(trunc_pos).collect::<String>()
    } else {
        let chars: Vec<char> = s.chars().take(trunc_pos).collect();
        match chars.iter().rposition(|&c| c == ' ') {
            Some(last_space) => chars[..last_space].iter().collect(),
            None => chars.iter().collect(),
        }
    };
    let mut result = String::with_capacity(truncated.len() + end.len());
    result.push_str(&truncated);
    result.push_str(end);
    Ok(result)
}

/// Counts the words in a string.
///
/// ```jinja
/// {{ "Hello world!"|wordcount }}
/// ```
#[cfg(feature = "wordcount")]
#[cfg_attr(docsrs, doc(cfg(feature = "wordcount")))]
pub fn wordcount(value: &Value) -> Result<Value, Error> {
    use unicode_categories::UnicodeCategories;

    let s = value.as_str().unwrap_or_default();
    let mut count: u32 = 0;
    let mut in_word = false;

    // Iterate through characters, counting transitions from non-word to word chars
    for c in s.chars() {
        let is_word_char = c.is_letter() || c.is_numeric() || c == '_';
        if is_word_char && !in_word {
            count += 1;
            in_word = true;
        } else if !is_word_char {
            in_word = false;
        }
    }

    Ok(Value::from(count))
}

/// Wrap a string to the given width.
///
/// By default this filter is not unicode aware (feature = `wordwrap`) but when the unicode
/// feature is enabled (`unicode_wordwrap`) then it becomes so.  It's implemented on top of
/// the `textwrap` crate.
///
/// **Keyword arguments:**
///
/// - `width`: Maximum length of wrapped lines (default: 79)
/// - `break_long_words`: If a word is longer than width, break it across lines (default: true)
/// - `break_on_hyphens`: If a word contains hyphens, it may be split across lines (default: true)
/// - `wrapstring`: String to join each wrapped line (default: newline)
#[cfg(feature = "wordwrap")]
#[cfg_attr(docsrs, doc(any(cfg(feature = "wordwrap"), cfg = "unicode_wordwrap")))]
pub fn wordwrap(value: &Value, kwargs: Kwargs) -> Result<Value, Error> {
    use textwrap::{wrap, Options as WrapOptions, WordSplitter};
    let s = value.as_str().unwrap_or_default();

    let width = kwargs.get::<Option<usize>>("width")?.unwrap_or(79);
    let break_long_words = kwargs
        .get::<Option<bool>>("break_long_words")?
        .unwrap_or(true);
    let break_on_hyphens = kwargs
        .get::<Option<bool>>("break_on_hyphens")?
        .unwrap_or(true);
    let wrapstring = kwargs.get::<Option<&str>>("wrapstring")?.unwrap_or("\n");
    kwargs.assert_all_used()?;

    let mut options = WrapOptions::new(width);

    if break_on_hyphens {
        options = options
            .word_splitter(WordSplitter::HyphenSplitter)
            .break_words(break_long_words);
    } else {
        // When break_on_hyphens is false, we want to preserve hyphenated words entirely.
        // So we disable hyphen splitting and also disable breaking long words to ensure
        // hyphenated words stay together.
        options = options
            .word_splitter(WordSplitter::NoHyphenation)
            .break_words(false);
    }

    // Handle empty/whitespace-only input
    if s.trim().is_empty() {
        return Ok(Value::from(""));
    }

    // Process paragraphs sequentially into final string
    Ok(Value::from(s.lines().enumerate().fold(
        String::new(),
        |mut acc, (i, p)| {
            if i > 0 {
                acc.push_str(wrapstring);
            }
            if !p.trim().is_empty() {
                // Wrap the paragraph and join with wrapstring
                let wrapped = wrap(p, &options);
                for (j, line) in wrapped.iter().enumerate() {
                    if j > 0 {
                        acc.push_str(wrapstring);
                    }
                    acc.push_str(line);
                }
            }
            acc
        },
    )))
}

/// Performs HTML tag stripping and unescaping.
///
/// ```jinja
/// {{ "<span>Hello &amp; World</span>"|striptags }} -> Hello & World
/// ```
///
/// By default the filter only knows about `&amp;`, `&lt;`, `&gt;`, and `&amp;`.  To
/// get all HTML5 entities, you need to enable the `html_entities` feature.
pub fn striptags(s: String) -> String {
    #[derive(Copy, Clone, PartialEq)]
    enum State {
        Text,
        TagStart,
        Tag,
        Entity,
        CommentStart1,
        CommentStart2,
        Comment,
        CommentEnd1,
        CommentEnd2,
    }

    let mut rv = String::new();
    let mut entity_buffer = String::new();
    let mut state = State::Text;

    macro_rules! push_char {
        ($c:expr) => {
            if $c.is_whitespace() {
                if rv.ends_with(|c: char| !c.is_whitespace()) {
                    rv.push(' ');
                }
            } else {
                rv.push($c);
            }
        };
    }

    for c in s.chars().map(Some).chain(Some(None)) {
        state = match (c, state) {
            (Some('<'), State::Text) => State::TagStart,
            (Some('>'), State::Tag | State::TagStart) => State::Text,
            (Some('!'), State::TagStart) => State::CommentStart1,
            (Some('-'), State::CommentStart1) => State::CommentStart2,
            (Some('-'), State::CommentStart2) => State::Comment,
            (_, State::CommentStart1 | State::CommentStart2) => State::Tag,
            (_, State::Tag | State::TagStart) => State::Tag,
            (Some('&'), State::Text) => {
                entity_buffer.clear();
                State::Entity
            }
            (Some('-'), State::Comment) => State::CommentEnd1,
            (Some('-'), State::CommentEnd1) => State::CommentEnd2,
            (Some('>'), State::CommentEnd2) => State::Text,
            (_, State::CommentEnd1 | State::CommentEnd2) => State::Comment,
            (_, State::Entity) => {
                let cc = c.unwrap_or('\x00');
                if cc == '\x00' || cc == ';' || cc == '<' || cc == '&' || cc.is_whitespace() {
                    if let Some(resolved) = resolve_numeric_entity(&entity_buffer) {
                        push_char!(resolved);
                    } else if let Ok(resolved) = HTML_ENTITIES
                        .binary_search_by_key(&entity_buffer.as_str(), |x| x.0)
                        .map(|x| &HTML_ENTITIES[x].1)
                    {
                        for c in resolved.chars() {
                            push_char!(c);
                        }
                    } else {
                        rv.push('&');
                        rv.push_str(&entity_buffer);
                        if cc == ';' {
                            rv.push(';');
                        }
                    }

                    if cc == '<' {
                        State::Tag
                    } else if cc == '&' {
                        entity_buffer.clear();
                        State::Entity
                    } else {
                        if cc.is_whitespace() {
                            push_char!(cc);
                        }
                        State::Text
                    }
                } else if let Some(c) = c {
                    entity_buffer.push(c);
                    State::Entity
                } else {
                    State::Entity
                }
            }
            (Some(c), State::Text) => {
                push_char!(c);
                State::Text
            }
            (_, state) => state,
        }
    }

    rv.truncate(rv.trim_end().len());

    rv
}

fn resolve_numeric_entity(entity: &str) -> Option<char> {
    let num_str = entity.strip_prefix('#')?;
    if num_str.starts_with('x') || num_str.starts_with('X') {
        let code = u32::from_str_radix(&num_str[1..], 16).ok()?;
        char::from_u32(code)
    } else if let Ok(code) = num_str.parse::<u32>() {
        char::from_u32(code)
    } else {
        None
    }
}

#[test]
fn test_entities_sorted() {
    assert!(HTML_ENTITIES.windows(2).all(|w| w[0] <= w[1]));
}
