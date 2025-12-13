use std::sync::LazyLock;

use anyhow::Result;

use super::ParseOptions;
use crate::jsonish::{
    parser::{entry, ParsingMode},
    Value,
};

#[derive(Debug)]
pub enum MarkdownResult {
    CodeBlock(String, Value),
    String(String),
}

// Cache compiled regexes for markdown parsing - these are compiled once and reused
static MD_TAG_START: LazyLock<regex::Regex> = LazyLock::new(|| {
    // Anchor fences to the start of a line (optionally indented) to avoid
    // confusing content like ```json that appears inside strings/code.
    regex::Regex::new(r"(?m)^[ \t]*```([a-zA-Z0-9 ]+)(?:\n|$)")
        .expect("Failed to compile md-tag-start regex")
});

static MD_TAG_END: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?m)^[ \t]*```(?:\n|$)").expect("Failed to compile md-tag-end regex")
});

pub fn parse(str: &str, options: &ParseOptions) -> Result<Vec<MarkdownResult>> {
    let mut values: Vec<MarkdownResult> = vec![];

    let mut remaining = str;

    let md_tag_start = &*MD_TAG_START;
    let md_tag_end = &*MD_TAG_END;

    let mut should_loop = true;

    while let Some(cap) = md_tag_start.find(remaining) {
        let tag = cap.as_str();
        log::trace!("Found tag: {cap:#?}");

        let after_start = &remaining[cap.end()..];

        // Heuristic: the first "```" after an opening fence might appear inside the fenced content
        // (e.g. within a JSON string). Prefer the last closing fence that yields a successful parse.
        let mut parsed_value: Option<Value> = None;

        let ends: Vec<_> = md_tag_end.find_iter(after_start).collect();
        let md_content = if ends.is_empty() {
            should_loop = false;
            after_start.trim()
        } else {
            // Prefer the first closing fence that yields a successful parse; this prevents us from
            // accidentally consuming subsequent fenced code blocks.
            let mut chosen_end = ends[0];
            let mut md_content = after_start[..chosen_end.start()].trim();

            for end in ends.iter() {
                let candidate = after_start[..end.start()].trim();
                if let Ok(v) = super::entry::parse_func(
                    candidate,
                    options.next_from_mode(ParsingMode::JsonMarkdown),
                    false,
                ) {
                    parsed_value = Some(v);
                    chosen_end = *end;
                    md_content = candidate;
                    break;
                }
            }

            remaining = &remaining[cap.end() + chosen_end.end()..];
            md_content
        };

        log::trace!("Content:\n-----\n{md_content}\n-----\n");

        let res = match parsed_value {
            Some(v) => Ok(v),
            None => super::entry::parse_func(
                md_content,
                options.next_from_mode(ParsingMode::JsonMarkdown),
                false,
            ),
        };

        match res {
            Ok(v) => {
                // TODO: Add any more additional strings here.
                values.push(MarkdownResult::CodeBlock(
                    {
                        let tag = tag.trim_start();
                        if tag.len() > 3 {
                            tag[3..].trim()
                        } else {
                            "<unspecified>"
                        }
                    }
                    .to_string(),
                    v,
                ));
            }
            Err(e) => {
                log::debug!("Error parsing markdown block: Tag: {tag}\n{e:?}");
            }
        };

        if !should_loop {
            break;
        }
    }

    if values.is_empty() {
        anyhow::bail!("No markdown blocks found")
    } else {
        if !remaining.trim().is_empty() {
            values.push(MarkdownResult::String(remaining.to_string()));
        }
        Ok(values)
    }
}

#[cfg(test)]
mod test {
    use baml_types::CompletionState;
    use test_log::test;

    use super::*;
    use crate::jsonish::Value;

    #[test]
    fn basic_parse() -> Result<()> {
        let res = parse(
            r#"```json
{
    "a": 1
}
```

Also we've got a few more!
```python
print("Hello, world!")
```

```test json
"This is a test"
```
"#,
            &ParseOptions::default(),
        );

        let res = res?;
        assert_eq!(res.len(), 2);
        {
            let (tag, value) = if let MarkdownResult::CodeBlock(tag, value) = &res[0] {
                (tag, value)
            } else {
                panic!("Expected CodeBlock, got {:#?}", res[0]);
            };
            assert_eq!(tag, "json");

            let Value::AnyOf(value, _) = value else {
                panic!("Expected AnyOf, got {value:#?}");
            };
            assert!(value.contains(&Value::Object(
                [(
                    "a".to_string(),
                    Value::Number((1).into(), CompletionState::Complete)
                )]
                .into_iter()
                .collect(),
                CompletionState::Complete
            )));
        }
        {
            let (tag, value) = if let MarkdownResult::CodeBlock(tag, value) = &res[1] {
                (tag, value)
            } else {
                panic!("Expected CodeBlock, got {:#?}", res[0]);
            };
            assert_eq!(tag, "test json");

            let Value::AnyOf(value, _) = value else {
                panic!("Expected AnyOf, got {value:#?}");
            };
            // dbg!(&value);
            assert!(value.contains(&Value::String(
                "This is a test".to_string(),
                CompletionState::Complete
            )));
        }

        Ok(())
    }

    #[test(should_panic)]
    fn untagged_blocks() -> Result<()> {
        let res = parse(
            r#"
lorem ipsum

```
"block1"
```

"here is some text in between"

```
"block2"
```

dolor sit amet
            "#,
            &ParseOptions::default(),
        );

        let res = res?;
        assert_eq!(res.len(), 2);

        Ok(())
    }

    #[test]
    fn utf8_between_blocks() -> Result<()> {
        let res = parse(
            r#"
lorem ipsum

```json
"block1"
```

🌅🌞🏖️🏊‍♀️🐚🌴🍹🌺🏝️🌊👒😎👙🩴🐠🚤🍉🎣🎨📸🎉💃🕺🌙🌠🍽️🎶✨🌌🏕️🔥🌲🌌🌟💤

```json
"block2"
```

dolor sit amet
            "#,
            &ParseOptions::default(),
        );

        let res = res?;
        assert_eq!(res.len(), 3);

        // Ensure the types of each.
        assert!(matches!(&res[0], MarkdownResult::CodeBlock(tag, _) if tag == "json"));
        assert!(matches!(&res[1], MarkdownResult::CodeBlock(tag, _) if tag == "json"));
        match &res[2] {
            MarkdownResult::String(s) => assert_eq!(s.trim(), "dolor sit amet"),
            _ => panic!("Expected String, got {:#?}", res[2]),
        }

        Ok(())
    }

    #[test]
    fn fence_like_sequence_inside_triple_backtick_string_does_not_split_markdown_blocks(
    ) -> Result<()> {
        let res = parse(
            r#"
```json
{
  "type": "code",
  "code": ```
  inside
  ```json
  not a markdown block
  ```,
}
```
"#,
            &ParseOptions::default(),
        )?;

        assert_eq!(res.len(), 1);
        let (tag, value) = match &res[0] {
            MarkdownResult::CodeBlock(tag, value) => (tag, value),
            _ => panic!("Expected CodeBlock, got {:#?}", res[0]),
        };
        assert_eq!(tag, "json");

        fn contains_substring(v: &Value, needle: &str) -> bool {
            match v {
                Value::String(s, _) => s.contains(needle),
                Value::Object(kvs, _) => kvs.iter().any(|(_, v)| contains_substring(v, needle)),
                Value::Array(items, _) => items.iter().any(|v| contains_substring(v, needle)),
                Value::Markdown(_, v, _) => contains_substring(v, needle),
                Value::FixedJson(v, _) => contains_substring(v, needle),
                Value::AnyOf(choices, original) => {
                    original.contains(needle)
                        || choices.iter().any(|v| contains_substring(v, needle))
                }
                Value::Number(_, _) | Value::Boolean(_) | Value::Null => false,
            }
        }

        // If markdown parsing were confused by the inner ```json line, we'd typically end up with
        // multiple markdown results or a malformed parsed value missing this substring.
        assert!(
            contains_substring(value, "```json\n  not a markdown block"),
            "Expected inner fence-like text preserved, got {value:#?}"
        );

        Ok(())
    }

    #[test]
    fn multiple_codeblocks_not_merged_when_fence_like_text_present() -> Result<()> {
        let res = parse(
            r#"
```json
{
  "type": "code",
  "code": ```
  first block
  ```json
  still content
  ```,
}
```

```json
{"type": "code", "code": "second block"}
```
"#,
            &ParseOptions::default(),
        )?;

        assert_eq!(res.len(), 2);
        assert!(matches!(&res[0], MarkdownResult::CodeBlock(tag, _) if tag == "json"));
        assert!(matches!(&res[1], MarkdownResult::CodeBlock(tag, _) if tag == "json"));
        Ok(())
    }
}
