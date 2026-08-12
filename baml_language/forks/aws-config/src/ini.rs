//! A minimal INI parser for AWS shared config / credentials files.
//!
//! Handles the subset of INI that AWS files use: `[section]` headers, `key =
//! value` entries, `#`/`;` line comments, and surrounding whitespace. Nested
//! sub-properties (indented keys under a parent key) are not used by the
//! credential sources BAML supports and are skipped.

use std::collections::HashMap;

/// A parsed INI file: ordered-insensitive map of section name → (key → value).
#[derive(Debug, Default, Clone)]
pub struct IniFile {
    sections: HashMap<String, HashMap<String, String>>,
}

impl IniFile {
    /// Parse INI text. Malformed lines are skipped rather than erroring, which
    /// matches the lenient behavior callers expect from config files.
    pub fn parse(text: &str) -> Self {
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current: Option<String> = None;

        for raw in text.lines() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    let name = name.trim().to_string();
                    sections.entry(name.clone()).or_default();
                    current = Some(name);
                }
                continue;
            }
            if let (Some(section), Some((key, value))) = (&current, line.split_once('=')) {
                let key = key.trim().to_string();
                let value = value.trim().to_string();
                if !key.is_empty() {
                    sections
                        .entry(section.clone())
                        .or_default()
                        .insert(key, value);
                }
            }
        }

        IniFile { sections }
    }

    /// Look up a section by name.
    pub fn section(&self, name: &str) -> Option<&HashMap<String, String>> {
        self.sections.get(name)
    }

    /// Iterate over `(section_name, entries)`.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &HashMap<String, String>)> {
        self.sections.iter()
    }
}

/// Strip a trailing `#` or `;` comment. Comments only start a comment when at
/// the beginning of the trimmed line or preceded by whitespace, matching the
/// AWS parser (so `url=a#b` keeps `#b`, but `key = value # note` drops it).
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut prev_ws = true; // start-of-line counts as preceded-by-whitespace
    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'#' || b == b';') && prev_ws {
            return &line[..i];
        }
        prev_ws = b == b' ' || b == b'\t';
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_and_keys() {
        let ini = IniFile::parse(
            "\
[default]
aws_access_key_id = AKIA
aws_secret_access_key = SECRET

[profile work]
region = us-west-2
",
        );
        assert_eq!(
            ini.section("default")
                .unwrap()
                .get("aws_access_key_id")
                .unwrap(),
            "AKIA"
        );
        assert_eq!(
            ini.section("profile work").unwrap().get("region").unwrap(),
            "us-west-2"
        );
    }

    #[test]
    fn strips_line_comments() {
        let ini = IniFile::parse(
            "\
# a comment
[default]
region = us-east-1 ; trailing comment
key = value # note
",
        );
        let d = ini.section("default").unwrap();
        assert_eq!(d.get("region").unwrap(), "us-east-1");
        assert_eq!(d.get("key").unwrap(), "value");
    }

    #[test]
    fn keeps_hash_without_preceding_space() {
        let ini = IniFile::parse("[s]\nurl = http://x/a#b\n");
        assert_eq!(
            ini.section("s").unwrap().get("url").unwrap(),
            "http://x/a#b"
        );
    }
}
