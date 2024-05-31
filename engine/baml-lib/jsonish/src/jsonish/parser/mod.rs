mod entry;
mod fixing_parser;
mod markdown_parser;
mod multi_json_parser;

pub use entry::parse;

#[derive(Clone, Copy, Debug)]
pub struct ParseOptions {
    all_finding_all_json_objects: bool,
    allow_markdown_json: bool,
    allow_fixes: bool,
    allow_as_string: bool,
    depth: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            all_finding_all_json_objects: true,
            allow_markdown_json: true,
            allow_fixes: true,
            allow_as_string: true,
            depth: 0,
        }
    }
}

enum ParsingMode {
    JsonMarkdown,
    AllJsonObjects,
}

impl ParseOptions {
    pub fn next_from_mode(&self, curr_mode: ParsingMode) -> Self {
        let mut new = self.clone();
        match curr_mode {
            ParsingMode::JsonMarkdown => {
                new.allow_markdown_json = false;
                new.allow_as_string = false;
            }
            ParsingMode::AllJsonObjects => {
                new.all_finding_all_json_objects = false;
                new.allow_as_string = false;
            }
        }
        new
    }
}
