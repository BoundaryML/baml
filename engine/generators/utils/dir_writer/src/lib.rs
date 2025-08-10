use std::{
    collections::BTreeMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use anyhow::Result;
use indexmap::IndexMap;
use internal_baml_core::configuration::{
    GeneratorDefaultClientMode, GeneratorOutputType, ModuleFormat,
};
pub use internal_baml_core::ir::repr::IntermediateRepr;

pub struct GeneratorArgs {
    /// Output directory for the generated client, relative to baml_src
    pub output_dir_relative_to_baml_src: PathBuf,

    /// Path to the BAML source directory
    pub baml_src_dir: PathBuf,

    pub inlined_file_map: BTreeMap<PathBuf, String>,

    pub version: String,
    pub no_version_check: bool,

    // Default call mode for functions
    pub default_client_mode: GeneratorDefaultClientMode,
    pub on_generate: Vec<String>,

    // The type of client to generate
    pub client_type: GeneratorOutputType,
    pub client_package_name: Option<String>,

    // For TS generators, we can choose between CJS and ESM module formats
    pub module_format: Option<ModuleFormat>,

    // for python, we can choose between pydantic 1 and pydantic 2
    pub is_pydantic_2: Option<bool>,
}

fn relative_path_to_baml_src(path: &Path, baml_src: &Path) -> Result<PathBuf> {
    pathdiff::diff_paths(path, baml_src).ok_or_else(|| {
        anyhow::anyhow!(
            "Failed to compute relative path from {} to {}",
            path.display(),
            baml_src.display()
        )
    })
}

impl GeneratorArgs {
    pub fn new<'i>(
        output_dir_relative_to_baml_src: impl Into<PathBuf>,
        baml_src_dir: impl Into<PathBuf>,
        input_files: impl IntoIterator<Item = (&'i PathBuf, &'i String)>,
        version: String,
        no_version_check: bool,
        default_client_mode: GeneratorDefaultClientMode,
        on_generate: Vec<String>,
        client_type: GeneratorOutputType,
        client_package_name: Option<String>,
        module_format: Option<ModuleFormat>,
    ) -> Result<Self> {
        let baml_src = baml_src_dir.into();
        let input_file_map: BTreeMap<PathBuf, String> = input_files
            .into_iter()
            .map(|(k, v)| Ok((relative_path_to_baml_src(k, &baml_src)?, v.clone())))
            .collect::<Result<_>>()?;

        Ok(Self {
            output_dir_relative_to_baml_src: output_dir_relative_to_baml_src.into(),
            baml_src_dir: baml_src.clone(),
            // for the key, whhich is the name, just get the filename
            inlined_file_map: input_file_map,
            version,
            no_version_check,
            default_client_mode,
            on_generate,
            client_type,
            client_package_name,
            module_format,
            is_pydantic_2: match client_type {
                GeneratorOutputType::PythonPydantic => Some(true),
                GeneratorOutputType::PythonPydanticV1 => Some(false),
                _ => None,
            },
        })
    }

    pub fn file_map_as_json_string(&self) -> Result<Vec<(String, String)>> {
        self.inlined_file_map
            .iter()
            .map(|(k, v)| {
                Ok((
                    serde_json::to_string(&k.display().to_string()).map_err(|e| {
                        anyhow::Error::from(e)
                            .context(format!("Failed to serialize key {:#}", k.display()))
                    })?,
                    serde_json::to_string(v).map_err(|e| {
                        anyhow::Error::from(e)
                            .context(format!("Failed to serialize contents of {:#}", k.display()))
                    })?,
                ))
            })
            .collect()
    }

    pub fn output_dir(&self) -> PathBuf {
        use sugar_path::SugarPath;
        self.baml_src_dir
            .join(&self.output_dir_relative_to_baml_src)
            .normalize()
    }

    /// Returns baml_src relative to the output_dir.
    ///
    /// We need this to be able to codegen a singleton client, so that our generated code can build
    /// baml_src relative to the path of the file in which the singleton is defined. In Python this is
    /// os.path.dirname(__file__) for globals.py; in TS this is __dirname for globals.ts.
    pub fn baml_src_relative_to_output_dir(&self) -> Result<PathBuf> {
        // for some reason our build requires this to be a borrow, but it's not a borrow in the
        // actual code
        #[allow(clippy::needless_borrows_for_generic_args)]
        pathdiff::diff_paths(&self.baml_src_dir, &self.output_dir()).ok_or_else(|| {
            anyhow::anyhow!(
                "Failed to compute baml_src ({}) relative to output_dir ({})",
                self.baml_src_dir.display(),
                self.output_dir().display()
            )
        })
    }
}

pub enum RemoveDirBehavior {
    /// Refuse to overwrite files that BAML did not generate
    ///
    /// This is the default
    Safe,

    /// Allow overwriting files that BAML did not generate
    ///
    /// Used by OpenAPI codegen, which runs openapi-generator and creates all sorts
    /// of files that BAML can't know about in advance
    Unsafe,
}

/// Controls output-type-specific behavior of codegen
pub trait LanguageFeatures: Default + Sized {
    const CONTENT_PREFIX: &'static str;

    fn name() -> &'static str;

    fn content_prefix(&self) -> &'static str {
        Self::CONTENT_PREFIX.trim()
    }

    fn on_file_created(&self, _path: &Path, content: &mut String) -> Result<()> {
        content.push_str(self.content_prefix());
        content.push('\n');
        Ok(())
    }

    fn on_file_finished(&self, _path: &Path, _content: &mut String) -> Result<()> {
        Ok(())
    }

    const REMOVE_DIR_BEHAVIOR: RemoveDirBehavior = RemoveDirBehavior::Safe;

    /// If set, the contents of a .gitignore file to be written to the generated baml_client
    ///
    /// It's only safe to set this for rest/openapi right now - still need to work out
    /// backwards compat implications for the other generators
    const GITIGNORE: Option<&'static str> = None;

    fn generate_sdk<'a>(
        &'a self,
        ir: std::sync::Arc<IntermediateRepr>,
        args: &GeneratorArgs,
    ) -> Result<IndexMap<PathBuf, String>, anyhow::Error> {
        let mut collector: FileCollector<'a, Self> = FileCollector::<'a, Self>::new();
        collector.on_file_created.push(Box::new(|path, content| {
            self.on_file_created(path, content)
        }));
        collector.on_file_finished.push(Box::new(|path, content| {
            self.on_file_finished(path, content)
        }));
        self.generate_sdk_files(&mut collector, ir, args)?;
        collector.commit(&args.output_dir())
    }

    fn generate_sdk_files_for_test<'a>(
        &'a self,
        ir: std::sync::Arc<IntermediateRepr>,
        args: &GeneratorArgs,
    ) -> Result<IndexMap<PathBuf, String>, anyhow::Error> {
        let mut collector: FileCollector<'a, Self> = FileCollector::<'a, Self>::new();
        collector.on_file_created.push(Box::new(|path, content| {
            self.on_file_created(path, content)
        }));
        collector.on_file_finished.push(Box::new(|path, content| {
            self.on_file_finished(path, content)
        }));
        self.generate_sdk_files(&mut collector, ir, args)?;
        Ok(collector.files)
    }

    fn generate_sdk_files(
        &self,
        collector: &mut FileCollector<Self>,
        ir: std::sync::Arc<IntermediateRepr>,
        args: &GeneratorArgs,
    ) -> Result<(), anyhow::Error>;
}

pub struct FileCollector<'a, L: LanguageFeatures + Default> {
    // map of path to a an object that has the trail File
    files: IndexMap<PathBuf, String>,
    lang: L,

    on_file_created: Vec<Box<dyn Fn(&Path, &mut String) -> Result<()> + 'a>>,
    on_file_finished: Vec<Box<dyn Fn(&Path, &mut String) -> Result<()> + 'a>>,
}

fn try_delete_tmp_dir(temp_path: &Path) -> Result<()> {
    // if the .tmp dir exists, delete it so we can get back to a working state without user intervention.
    let delete_attempts = 3; // Number of attempts to delete the directory
    let attempt_interval = Duration::from_millis(200); // Wait time between attempts

    for attempt in 1..=delete_attempts {
        if temp_path.exists() {
            match std::fs::remove_dir_all(temp_path) {
                Ok(_) => {
                    baml_log::debug!("Temp directory successfully removed.");
                    break; // Exit loop after successful deletion
                }
                Err(e) if e.kind() == ErrorKind::Other && attempt < delete_attempts => {
                    baml_log::warn!(
                        "Attempt {}: Failed to delete temp directory: {}",
                        attempt,
                        e
                    );
                    sleep(attempt_interval); // Wait before retrying
                }
                Err(e) => {
                    // For other errors or if it's the last attempt, fail with an error
                    return Err(anyhow::Error::new(e).context(format!(
                        "Failed to delete temp directory '{temp_path:?}' after {attempt} attempts"
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
    Ok(())
}

impl<'a, L: LanguageFeatures + Default> Default for FileCollector<'a, L> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, L: LanguageFeatures + Default> FileCollector<'a, L> {
    pub fn new() -> Self {
        Self {
            files: IndexMap::new(),
            lang: L::default(),
            on_file_created: vec![],
            on_file_finished: vec![],
        }
    }

    fn on_file_created(&mut self, path: &Path) -> Result<()> {
        let mut content = Default::default();
        for on_file_created in self.on_file_created.iter() {
            on_file_created(path, &mut content)?;
        }
        self.files.insert(path.to_path_buf(), content);
        Ok(())
    }

    pub fn add_file<K: AsRef<str>, V: AsRef<str>>(&mut self, name: K, contents: V) -> Result<()> {
        if self.files.contains_key(&PathBuf::from(name.as_ref())) {
            anyhow::bail!("File already exists: {}", name.as_ref());
        }
        self.on_file_created(&PathBuf::from(name.as_ref()))?;
        self.append_to_file(name, contents.as_ref())?;
        Ok(())
    }

    pub fn append_to_file<K: AsRef<str>>(&mut self, name: K, contents: &str) -> Result<()> {
        let file = self
            .files
            .get_mut(&PathBuf::from(name.as_ref()))
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", name.as_ref()))?;
        file.push('\n');
        file.push_str(contents);
        Ok(())
    }

    pub fn modify_files(&mut self, mut modify: impl FnMut(&mut String)) {
        for (_path, content) in self.files.iter_mut() {
            modify(content);
        }
    }

    /// Ensure that a directory contains only files we generated before nuking it.
    ///
    /// This is a safety measure to prevent accidentally deleting user files.
    ///
    /// We consider a file to be "generated by BAML" if it contains "generated by BAML"
    /// in the first 1024 bytes, and limit our search to a max of N unrecognized files.
    /// This gives us performance bounds if, for example, we find ourselves iterating
    /// through node_modules or .pycache or some other thing.
    fn remove_dir_safe(&self, output_path: &Path) -> Result<()> {
        if !output_path.exists() {
            return Ok(());
        }

        const MAX_UNKNOWN_FILES: usize = 4;
        let mut unknown_files = vec![];
        for entry in walkdir::WalkDir::new(output_path)
            .into_iter()
            .filter_entry(|e| e.path().file_name().is_some_and(|f| f != "__pycache__"))
        {
            if unknown_files.len() > MAX_UNKNOWN_FILES {
                break;
            }
            let entry = entry?;
            if entry.file_type().is_dir() {
                // Only files matter for the pre-existence check
                continue;
            }
            let path = entry.path();
            if let Ok(mut f) = std::fs::File::open(path) {
                use std::io::Read;
                let mut buf = [0; 1024];
                if f.read(&mut buf).is_ok()
                    && String::from_utf8_lossy(&buf).contains("generated by BAML")
                {
                    continue;
                }
            }
            let path = path.strip_prefix(output_path)?.to_path_buf();
            unknown_files.push(path);
        }
        unknown_files.sort();
        match L::REMOVE_DIR_BEHAVIOR {
            RemoveDirBehavior::Safe => match unknown_files.len() {
                0 => (),
                1 => anyhow::bail!(
                    "output directory contains a file that BAML did not generate\n\n\
                Please remove it and re-run codegen.\n\n\
                File: {}",
                    output_path.join(&unknown_files[0]).display()
                ),
                n => {
                    if n < MAX_UNKNOWN_FILES {
                        anyhow::bail!(
                            "output directory contains {n} files that BAML did not generate\n\n\
                    Please remove them and re-run codegen.\n\n\
                    Files:\n{}",
                            unknown_files
                                .iter()
                                .map(|p| format!("  - {}", output_path.join(p).display()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
                    } else {
                        anyhow::bail!(
                        "output directory contains at least {n} files that BAML did not generate\n\n\
                    Please remove all files not generated by BAML and re-run codegen.\n\n\
                    Files:\n{}",
                        unknown_files
                            .iter()
                            .map(|p| format!("  - {}", output_path.join(p).display()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                    }
                }
            },
            RemoveDirBehavior::Unsafe => {}
        }
        std::fs::remove_dir_all(output_path)?;
        Ok(())
    }

    /// Commit the generated files to disk.
    ///
    /// Writes to the output path, and returns a map of the paths to the contents.
    /// Ensures that we don't stomp on user files.
    ///
    /// `output_path` is the path to be written to, and the path that will be prepended
    /// to the returned file entries
    pub fn commit(&mut self, output_path: &Path) -> Result<IndexMap<PathBuf, String>> {
        for (path, content) in self.files.iter_mut() {
            for on_file_finished in self.on_file_finished.iter() {
                on_file_finished(path, content)?;
            }
        }

        if let Some(gitignore) = L::GITIGNORE {
            self.files.insert(
                PathBuf::from(".gitignore"),
                format!("{}\n", gitignore.trim_start()),
            );
        }

        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                baml_log::debug!("Committing generated files in wasm is a no-op (writing is the Nodejs caller's responsibility)");
            } else {
                baml_log::debug!("Writing files to {}", output_path.display());

                let temp_path = PathBuf::from(format!("{}.tmp", output_path.display()));

                // if the .tmp dir exists, delete it so we can get back to a working state without user intervention.
                try_delete_tmp_dir(temp_path.as_path())?;

                // Sort the files by path so that we always write to the same file
                for (relative_file_path, contents) in self.files.iter() {
                    let full_file_path = temp_path.join(relative_file_path);
                    if let Some(parent) = full_file_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(&full_file_path, contents)?;
                }

                self.remove_dir_safe(output_path)?;
                std::fs::rename(&temp_path, output_path)?;

                // Update file modification times to trigger file watchers
                // Note we maay not have to do this now that we use Rust always to
                // generate files. But we'll keep it for now.
                let now = filetime::FileTime::now();
                for relative_file_path in self.files.keys() {
                    let full_file_path = output_path.join(relative_file_path);
                    if let Err(e) = filetime::set_file_mtime(&full_file_path, now) {
                        // Log a warning but don't fail the whole process if touching fails
                        baml_log::warn!(
                            "Failed to update modification time for {}: {}",
                            full_file_path.display(),
                            e
                        );
                    }
                }

                baml_log::info!(
                    "Wrote {} files to {}",
                    self.files.len(),
                    output_path.display()
                );
            }
        }

        Ok(self.files.clone())
    }
}
