use crate::jsonish::{
    parser::{entry, ParsingMode},
    Value,
};

use super::ParseOptions;
use anyhow::Result;

#[derive(Debug)]
pub enum MarkdownResult {
    CodeBlock(String, Value),
    String(String),
}

pub fn parse<'a>(str: &'a str, options: &ParseOptions) -> Result<Vec<MarkdownResult>> {
    let mut values = vec![];

    let mut remaining = str;
    // Find regex for markdown blocks (```<tag><EOF|newline>)

    let md_tag_start = regex::Regex::new(r"```([a-zA-Z0-9 ]+)(?:\n|$)")
        .map_err(|e| anyhow::Error::from(e).context("Failed to build regex for md-tag-start"))?;
    let md_tag_end = regex::Regex::new(r"```(?:\n|$)")
        .map_err(|e| anyhow::Error::from(e).context("Failed to build regex for md-tag-end"))?;

    let mut should_loop = true;

    while let Some(cap) = md_tag_start.find(remaining) {
        let tag = cap.as_str();
        log::trace!("Found tag: {:#?}", cap);

        let md_content = if let Some(end) = md_tag_end.find(&remaining[cap.end()..]) {
            let next = remaining[cap.end()..cap.end() + end.start()].trim();
            remaining = &remaining[cap.end() + end.end()..];
            next
        } else {
            should_loop = false;
            remaining[cap.end()..].trim()
        };

        log::trace!("Content:\n-----\n{}\n-----\n", md_content);

        let res = entry::parse(
            md_content,
            options.next_from_mode(ParsingMode::JsonMarkdown),
        );

        match res {
            Ok(v) => {
                // TODO: Add any more additional strings here.
                values.push(MarkdownResult::CodeBlock(
                    if tag.len() > 3 {
                        tag[3..].trim()
                    } else {
                        "<unspecified>"
                    }
                    .to_string(),
                    v,
                ));
            }
            Err(e) => {
                log::debug!("Error parsing markdown block: Tag: {tag}\n{:?}", e);
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
    use super::*;
    use test_log::test;

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
                panic!("Expected AnyOf, got {:#?}", value);
            };
            assert!(value.contains(&Value::Object(
                [("a".to_string(), Value::Number((1).into()))]
                    .into_iter()
                    .collect()
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
                panic!("Expected AnyOf, got {:#?}", value);
            };
            assert!(value.contains(&Value::String("This is a test".to_string())));
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
}
