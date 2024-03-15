use std::io::{self, ErrorKind};
use std::thread::sleep;
use std::time::Duration;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use log::info;

#[derive(PartialEq, Eq, Hash)]
pub(super) struct LibImport {
    pub lib: String,
    pub as_name: Option<String>,
}

#[derive(PartialEq, Eq, Hash)]
pub(super) struct Import {
    pub lib: String,
    pub name: String,
    pub as_name: Option<String>,
}

pub(super) struct FileContent {
    content: Vec<String>,
    import_libs: HashSet<LibImport>,
    imports: Vec<Import>,
    exports: Vec<String>,
    export_all: bool,
}

impl FileContent {
    pub fn add_import_lib(&mut self, lib: impl Into<String>, as_name: Option<&str>) {
        self.import_libs.insert(LibImport {
            lib: lib.into(),
            as_name: as_name.map(|s| s.to_string()),
        });
    }

    pub fn add_import(
        &mut self,
        lib: impl Into<String>,
        name: impl AsRef<str>,
        as_name: Option<&str>,
        is_export: bool,
    ) {
        self.imports.push(Import {
            lib: lib.into(),
            name: name.as_ref().to_string(),
            as_name: as_name.map(|s| s.to_string()),
        });
        if is_export || self.export_all {
            self.exports
                .push(as_name.unwrap_or(name.as_ref()).to_string());
        }
    }

    pub fn add_export(&mut self, name: impl Into<String>) {
        self.exports.push(name.into());
    }

    pub fn append(&mut self, content: String) {
        let content = content.trim();
        if content.len() > 0 {
            self.content.push(content.to_string());
        }
    }
}

pub(super) struct FileCollector<L: LanguageFeatures> {
    // map of path to a an object that has the trail File
    files: HashMap<PathBuf, FileContent>,
    /// This is used to keep track of the last file that was written to
    /// Useful for catching bugs to ensure that we don't write to the same file twice.
    last_file: Option<PathBuf>,

    lang: L,
}

impl<L: LanguageFeatures> FileCollector<L> {
    pub(super) fn new(lang: L) -> Self {
        Self {
            files: HashMap::new(),
            last_file: None,
            lang,
        }
    }

    pub(super) fn finish_file(&mut self) {
        assert!(self.last_file.is_some(), "No file to finish!");
        self.last_file = None;
    }

    pub(super) fn start_file<T: AsRef<str>>(
        &mut self,
        dir: &str,
        name: T,
        export_all: bool,
    ) -> &mut FileContent {
        assert!(
            self.last_file.is_none(),
            "Cannot start a new file before finishing the last one!"
        );

        let path = self.lang.to_file_path(dir, name.as_ref());
        self.last_file = Some(path.clone());
        self.files.entry(path).or_insert_with(|| FileContent {
            content: vec![],
            import_libs: HashSet::new(),
            imports: vec![],
            exports: vec![],
            export_all,
        })
    }

    pub(super) fn commit(&self, dir: &PathBuf) -> std::io::Result<()> {
        let output_path = dir;
        // info!("Writing files to {}", output_path.to_string_lossy());

        let temp_path = PathBuf::from(format!("{}.tmp", output_path.to_string_lossy().to_string()));

        // if the .tmp dir exists, delete it so we can get back to a working state without user intervention.
        let delete_attempts = 3; // Number of attempts to delete the directory
        let attempt_interval = Duration::from_millis(200); // Wait time between attempts

        for attempt in 1..=delete_attempts {
            if temp_path.exists() {
                match std::fs::remove_dir_all(&temp_path) {
                    Ok(_) => {
                        println!("Temp directory successfully removed.");
                        break; // Exit loop after successful deletion
                    }
                    Err(e) if e.kind() == ErrorKind::Other && attempt < delete_attempts => {
                        info!(
                            "Attempt {}: Failed to delete temp directory: {}",
                            attempt, e
                        );
                        sleep(attempt_interval); // Wait before retrying
                    }
                    Err(e) => {
                        // For other errors or if it's the last attempt, fail with an error
                        return Err(io::Error::new(
                            e.kind(),
                            format!("Failed to delete temp directory: {}", e),
                        ));
                    }
                }
            } else {
                break;
            }
        }

        if temp_path.exists() {
            // If the directory still exists after the loop, return an error
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to delete existing temp directory within the timeout",
            ));
        }

        // Sort the files by path so that we always write to the same file
        let mut files = self.files.iter().collect::<Vec<_>>();
        files.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (relative_file_path, file) in &files {
            let full_file_path = temp_path.join(relative_file_path);
            std::fs::create_dir_all(full_file_path.parent().unwrap())?;
            std::fs::write(&full_file_path, &self.format_file(file))?;
        }

        let _ = std::fs::remove_dir_all(dir);
        let res = std::fs::rename(&temp_path, output_path);

        info!("Wrote {} files to {}", files.len(), dir.display());
        res
    }

    fn format_file(&self, content: &FileContent) -> String {
        let mut result = vec![];

        if self.lang.content_prefix().len() > 0 {
            result.push(self.lang.content_prefix().to_string());
        }

        if content.imports.len() + content.import_libs.len() > 0 {
            result.push(
                self.lang
                    .format_imports(&content.import_libs, &content.imports),
            );
        }

        if content.content.len() > 0 {
            result.push(content.content.join("\n\n") + "\n");
        }

        if content.exports.len() > 0 {
            result.push(self.lang.format_exports(&content.exports));
        }

        result.join("\n\n") + "\n"
    }
}

// Add a trait per language that can be used to convert an Import into a string
pub(super) trait LanguageFeatures {
    fn to_file_path(&self, path: &str, name: &str) -> PathBuf;
    fn format_imports(&self, libs: &HashSet<LibImport>, import: &Vec<Import>) -> String;
    fn format_exports(&self, exports: &Vec<String>) -> String;
    fn content_prefix(&self) -> &'static str;
}

pub(super) trait WithFileContent<L: LanguageFeatures> {
    fn file_dir(&self) -> &'static str;
    fn file_name(&self) -> String;
    fn write(&self, fc: &mut FileCollector<L>);
}

// Until rust supports trait specialization, we can't implement a trait for the same type twice, even if it's generic and the generic type is different (e.g. a diff language).
// See https://users.rust-lang.org/t/multiple-trait-implementations-based-on-generic-type-bound/17064
// So to fix we have to hack around and actually have a *different* trait for Python
pub(super) trait WithFileContentPy<L: LanguageFeatures> {
    fn file_dir(&self) -> &'static str;
    fn file_name(&self) -> String;
    fn write(&self, fc: &mut FileCollector<L>);
}
