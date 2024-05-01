use anyhow::Result;
use std::io::ErrorKind;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use std::{collections::HashMap, path::PathBuf};

// Add a trait per language that can be used to convert an Import into a string
pub(super) trait LanguageFeatures {
    fn content_prefix(&self) -> &'static str;
    fn to_file_path(&self, path: &str) -> PathBuf;
}

pub(super) struct FileCollector<L: LanguageFeatures + Default> {
    // map of path to a an object that has the trail File
    files: HashMap<PathBuf, String>,

    lang: L,
}

impl<L: LanguageFeatures + Default> FileCollector<L> {
    pub(super) fn new() -> Self {
        Self {
            files: HashMap::new(),
            lang: L::default(),
        }
    }

    pub(super) fn add_file<K: AsRef<str>, V: AsRef<str>>(&mut self, name: K, contents: V) {
        self.files.insert(
            PathBuf::from(name.as_ref()),
            format!("{}\n{}", self.lang.content_prefix(), contents.as_ref()),
        );
    }

    pub(super) fn commit(&self, dir: &Path) -> Result<()> {
        let output_path = dir;
        log::debug!("Writing files to {}", output_path.to_string_lossy());

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
                        log::warn!(
                            "Attempt {}: Failed to delete temp directory: {}",
                            attempt,
                            e
                        );
                        sleep(attempt_interval); // Wait before retrying
                    }
                    Err(e) => {
                        // For other errors or if it's the last attempt, fail with an error
                        return Err(anyhow::Error::new(e).context(format!(
                            "Failed to delete temp directory '{:?}' after {} attempts",
                            temp_path, attempt
                        )));
                    }
                }
            } else {
                break;
            }
        }

        if temp_path.exists() {
            // If the directory still exists after the loop, return an error
            anyhow::bail!(
                "Failed to delete existing temp directory '{:?}' within the timeout",
                temp_path
            );
        }

        // Sort the files by path so that we always write to the same file
        let mut files = self.files.iter().collect::<Vec<_>>();
        files.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (relative_file_path, contents) in files.iter() {
            let full_file_path = temp_path.join(relative_file_path);
            if let Some(parent) = full_file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full_file_path, contents)?;
        }

        let _ = std::fs::remove_dir_all(dir);
        std::fs::rename(&temp_path, output_path)?;

        log::info!("Wrote {} files to {}", files.len(), dir.display());
        Ok(())
    }
}
