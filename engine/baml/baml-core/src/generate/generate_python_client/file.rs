use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use super::traits::WithFile;
use log::info;

const TAB_SIZE: usize = 4;
const TAB: &str = "                                                                  ";

#[derive(Debug, Default)]
pub(super) struct FileCollector {
    files: HashMap<PathBuf, File>,
}

impl FileCollector {
    pub fn add_file(&mut self, obj: impl WithFile) -> &mut File {
        let file = obj.file();
        let key = file.path.join(&file.name);
        self.files.insert(key.clone(), file);
        self.files.get_mut(&key).unwrap()
    }

    pub fn write(&self, output: &Option<String>) {
        for file in self.files.values() {
            let path = match output {
                Some(output) => PathBuf::from(output).join(&file.path),
                None => file.path.clone(),
            };
            info!("Writing file: {:?}", path);
            // std::fs::create_dir_all(&path).unwrap();
            // std::fs::write(path.join(&file.name), &file.content).unwrap();
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct File {
    path: PathBuf,
    name: String,
    content: String,
    imports: HashMap<String, HashSet<String>>,
}

fn clean_file_name(name: impl AsRef<str>) -> String {
    info!("clean_file_name: {:?}", name.as_ref());
    name.as_ref()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

impl File {
    pub fn new(path: impl AsRef<str>, name: impl AsRef<str>) -> Self {
        Self {
            path: clean_file_name(path).into(),
            name: format!("{}.py", clean_file_name(name)),
            content: String::new(),
            imports: HashMap::new(),
        }
    }

    pub fn add_import(&mut self, module: &str, name: &str) {
        self.imports
            .entry(module.to_string())
            .or_default()
            .insert(name.to_string());
    }

    pub fn add_line(&mut self, line: impl AsRef<str>) {
        self.add_indent_line(line, 0);
    }

    pub fn add_indent_line<'a>(&mut self, line: impl AsRef<str>, indent: usize) {
        self.add_indent_string(line, indent);
        self.add_empty_line();
    }

    pub fn add_indent_string(&mut self, string: impl AsRef<str>, indent: usize) {
        let num_spaces = indent * TAB_SIZE;

        let prefix = match num_spaces > TAB.len() {
            true => panic!("Indentation too large"),
            false => &TAB[..num_spaces],
        };
        // Split the string by newlines and add each line with the correct indent
        let mut lines = string.as_ref().split('\n').peekable();
        // Loop through the lines
        while let Some(line) = lines.next() {
            // Add the prefix if the line is not empty
            if !line.is_empty() {
                self.add_string(&prefix);
            }

            // Add the line itself
            self.add_string(line);

            // Add an empty line if there are more lines remaining
            if lines.peek().is_some() {
                self.add_empty_line();
            }
        }
    }

    pub fn add_empty_line(&mut self) {
        self.content.push('\n');
    }

    pub fn add_string(&mut self, string: impl AsRef<str>) {
        self.content.push_str(string.as_ref());
    }
}
