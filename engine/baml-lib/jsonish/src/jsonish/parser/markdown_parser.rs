use crate::jsonish::{
    parser::{entry, ParsingMode},
    Value,
};

use super::ParseOptions;
use anyhow::Result;

pub fn parse<'a>(str: &'a str, options: &ParseOptions) -> Result<Vec<(String, Value)>> {
    let mut values = vec![];

    let mut remaining = str;
    // Find regex for markdown blocks (```<tag><EOF|newline>)

    let md_tag_start = regex::Regex::new(r"```([a-zA-Z0-9 ]+)(?:\n|$)").expect("Invalid regex");
    let md_tag_end = regex::Regex::new(r"```(?:\n|$)").expect("Invalid regex");

    let mut should_loop = true;

    while let Some(cap) = md_tag_start.find(remaining) {
        let tag = cap.as_str();
        log::info!("Found tag: {:#?}", cap);

        let md_content = if let Some(end) = md_tag_end.find(&remaining[cap.end()..]) {
            let next = remaining[cap.end()..cap.end() + end.start()].trim();
            remaining = &remaining[end.end()..];
            next
        } else {
            should_loop = false;
            remaining[cap.end()..].trim()
        };

        log::info!("Content:\n-----\n{}\n-----\n", md_content);

        let res = entry::parse(
            md_content,
            options.next_from_mode(ParsingMode::JsonMarkdown),
        );

        match res {
            Ok(v) => {
                values.push((
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
        Ok(values)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use test_log::test;

    #[test]
    fn test_parse() {
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

        assert!(res.is_ok(), "{:?}", res);

        let res = res.unwrap();
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0, "json");
        assert_eq!(res[1].0, "test json");
        assert_eq!(
            res[0].1,
            Value::Object(
                [("a".to_string(), Value::Number((1).into()))]
                    .into_iter()
                    .collect()
            )
        );
        assert_eq!(res[1].1, Value::String("This is a test".to_string()));
    }
}
