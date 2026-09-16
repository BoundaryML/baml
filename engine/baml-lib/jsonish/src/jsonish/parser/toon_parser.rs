use std::collections::HashSet;

use toon_format::{decode, DecodeOptions};

use crate::jsonish::Value;

/// Decode structured TOON candidates before falling back to JSON repair.
///
/// LLMs often prepend a short explanation, so in addition to the complete
/// response we try suffixes beginning at a TOON object field or array header.
/// Primitive-only responses are already handled well by JSONish and are not
/// considered here, which avoids interpreting arbitrary prose as TOON.
pub(crate) fn parse(input: &str) -> Option<Vec<Value>> {
    let lines = input.lines().collect::<Vec<_>>();
    let mut candidates = Vec::<String>::new();

    if let Some(index) = find_toon_start(&lines) {
        candidates.push(lines[index..].join("\n"));
    }

    candidates.extend(fenced_toon_blocks(input).into_iter().map(str::to_string));

    let options = DecodeOptions::new().with_strict(false);
    let mut seen = HashSet::new();
    let values = candidates
        .into_iter()
        .map(|candidate| dedent(&candidate))
        .map(|candidate| repair_inline_array_lengths(&candidate))
        .filter_map(|candidate| decode::<serde_json::Value>(&candidate, &options).ok())
        .filter(|value| value.is_object() || value.is_array())
        .filter_map(|value| serde_json::from_value::<Value>(value).ok())
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

fn find_toon_start(lines: &[&str]) -> Option<usize> {
    let mut start = None;
    for (index, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.starts_with('{') || (line.starts_with('[') && !looks_like_toon_header(line)) {
            return None;
        }
        if start.is_none() && looks_like_toon(line) {
            start = Some(index);
        }
    }

    start
}

fn dedent(input: &str) -> String {
    let indent = input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()
        .unwrap_or(0);

    input
        .lines()
        .map(|line| line.get(indent..).unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn repair_inline_array_lengths(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            let Some(open) = line.find('[') else {
                return line.to_string();
            };
            let Some(relative_close) = line[open + 1..].find("]: ") else {
                return line.to_string();
            };
            let close = open + 1 + relative_close;
            let header = &line[open + 1..close];
            let (length, delimiter) = match header.chars().last() {
                Some('|') => (&header[..header.len() - 1], '|'),
                Some('\t') => (&header[..header.len() - 1], '\t'),
                _ => (header, ','),
            };
            if length.is_empty() || !length.chars().all(|c| c.is_ascii_digit()) {
                return line.to_string();
            }

            let values = &line[close + 3..];
            if values.is_empty() {
                return line.to_string();
            }
            let actual = count_delimited_values(values, delimiter);
            let delimiter_marker = match delimiter {
                ',' => "",
                '|' => "|",
                '\t' => "\t",
                _ => unreachable!("TOON only supports comma, pipe, and tab delimiters"),
            };
            format!(
                "{}[{}{}]{}",
                &line[..open],
                actual,
                delimiter_marker,
                &line[close + 1..]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn count_delimited_values(values: &str, delimiter: char) -> usize {
    let mut count = 1;
    let mut quoted = false;
    let mut escaped = false;

    for ch in values.chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' && quoted {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch == delimiter && !quoted {
            count += 1;
        }
    }

    count
}

fn looks_like_toon(line: &str) -> bool {
    let line = line.trim();
    if line.is_empty() || line.starts_with('{') {
        return false;
    }

    if looks_like_toon_header(line) {
        return true;
    }

    line.split_once(':')
        .is_some_and(|(key, _)| is_valid_toon_key(key.trim()))
}

fn looks_like_toon_header(line: &str) -> bool {
    let Some((header, _)) = line.split_once(':') else {
        return false;
    };
    let Some(open) = header.find('[') else {
        return false;
    };
    let Some(relative_close) = header[open + 1..].find(']') else {
        return false;
    };
    let close = open + 1 + relative_close;
    let key = header[..open].trim();
    if !key.is_empty() && !is_valid_toon_key(key) {
        return false;
    }

    let length = header[open + 1..close].trim_end_matches(['|', '\t']);
    if length.is_empty() || !length.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    let fields = header[close + 1..].trim();
    fields.is_empty() || (fields.starts_with('{') && fields.ends_with('}'))
}

fn is_valid_toon_key(key: &str) -> bool {
    if key.starts_with('"') && key.ends_with('"') && key.len() >= 2 {
        return true;
    }

    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
}

fn fenced_toon_blocks(input: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut offset = 0;

    while let Some(open) = find_line_start_fence(input, offset) {
        let after_open = open + 3;
        let Some(line_end) = input[after_open..].find('\n') else {
            break;
        };
        let tag = input[after_open..after_open + line_end].trim();
        let content_start = after_open + line_end + 1;
        let Some(content_end) = find_line_start_fence(input, content_start) else {
            break;
        };

        if tag.is_empty() || tag.eq_ignore_ascii_case("toon") {
            blocks.push(input[content_start..content_end].trim());
        }
        offset = content_end + 3;
    }

    blocks
}

fn find_line_start_fence(input: &str, offset: usize) -> Option<usize> {
    input[offset..]
        .match_indices("```")
        .map(|(index, _)| offset + index)
        .find(|&index| {
            let line_start = input[..index]
                .rfind('\n')
                .map_or(0, |line_end| line_end + 1);
            input[line_start..index].chars().all(char::is_whitespace)
        })
}

#[cfg(test)]
mod tests {
    use baml_types::CompletionState;

    use super::*;

    #[test]
    fn parses_toon_after_preamble() {
        let values = parse("Here is the result:\nname: Ada\ntags[2]: math,code").unwrap();
        assert!(values.iter().any(|value| matches!(
            value,
            Value::Object(fields, _) if *fields == vec![
                ("name".into(), Value::String("Ada".into(), CompletionState::Complete)),
                ("tags".into(), Value::Array(
                    vec![
                        Value::String("math".into(), CompletionState::Complete),
                        Value::String("code".into(), CompletionState::Complete),
                    ],
                    CompletionState::Complete,
                )),
            ]
        )));
    }

    #[test]
    fn recovers_from_an_incorrect_array_length() {
        let values = parse("items[3]: one,two").unwrap();
        assert!(values.iter().any(|value| matches!(
            value,
            Value::Object(fields, _) if
                matches!(&fields[0].1, Value::Array(items, _) if items.len() == 2))));
    }

    #[test]
    fn ignores_plain_prose() {
        assert!(parse("This is only prose.").is_none());
    }

    #[test]
    fn parses_globally_indented_toon() {
        let values = parse("  name: Ada\n  metadata:\n    active: true").unwrap();
        assert!(values.iter().any(|value| matches!(
            value,
            Value::Object(fields, _) if fields.iter().any(|(key, _)| key == "name")
        )));
    }

    #[test]
    fn recognizes_keyed_and_root_array_headers() {
        assert!(looks_like_toon("people[2]{name,age}:"));
        assert!(looks_like_toon("[2]: one,two"));
        assert!(!looks_like_toon("not a key: value"));
    }

    #[test]
    fn does_not_scan_inside_json() {
        assert!(parse("{\n  \"name\": \"Ada\"\n}").is_none());
        assert!(parse("Result:\n{\n  \"name\": \"Ada\"\n}").is_none());
    }

    #[test]
    fn repairs_only_inline_array_lengths() {
        assert_eq!(
            repair_inline_array_lengths("items[4]: one,\"two,three\",four"),
            "items[3]: one,\"two,three\",four"
        );
        assert_eq!(
            repair_inline_array_lengths("items[2]{name,age}:\n  Ada,36"),
            "items[2]{name,age}:\n  Ada,36"
        );
    }

    #[test]
    fn ignores_inline_backticks_when_pairing_fenced_toon_blocks() {
        let blocks = fenced_toon_blocks("A stray ``` in prose.\n  ```toon\n  name: Ada\n  ```\n");

        assert_eq!(blocks, vec!["name: Ada"]);
    }
}
