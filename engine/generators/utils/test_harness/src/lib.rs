use std::{collections::BTreeMap, io::Write, path::PathBuf, process::Command, str::FromStr};

use dir_writer::{GeneratorArgs, IntermediateRepr, LanguageFeatures};
use internal_baml_core::ir::repr::make_test_ir_from_dir;

pub trait TestLanguageFeatures: LanguageFeatures {
    fn test_name() -> &'static str;
}

pub struct TestStructure<L: TestLanguageFeatures> {
    src_dir: PathBuf,
    ir: std::sync::Arc<IntermediateRepr>,
    generator: L,
    project_name: String,
    persist: bool,
}

fn get_cargo_root() -> Result<PathBuf, anyhow::Error> {
    let cargo_root = std::env::var("CARGO_MANIFEST_DIR")?;
    Ok(PathBuf::from(cargo_root).join("../../..").canonicalize()?)
}

fn get_dylib_path() -> Result<PathBuf, anyhow::Error> {
    // Prefer BAML_LIBRARY_PATH env var if set (used in CI to point to a stable copy)
    if let Ok(env_path) = std::env::var("BAML_LIBRARY_PATH") {
        let path = PathBuf::from(&env_path);
        eprintln!("[test-harness] Using BAML_LIBRARY_PATH: {}", path.display());
        if path.exists() {
            let size = std::fs::metadata(&path)?.len();
            eprintln!("[test-harness] File exists, size: {} bytes", size);
            if size < 1024 {
                anyhow::bail!(
                    "BAML_LIBRARY_PATH file is too small ({} bytes): {}",
                    size,
                    path.display()
                );
            }
            return Ok(path);
        } else {
            eprintln!(
                "[test-harness] Warning: BAML_LIBRARY_PATH set but file doesn't exist: {}",
                path.display()
            );
        }
    }

    // Fall back to cargo target directory
    let dylib_path = get_cargo_root()?
        .join("target/debug")
        .join(if cfg!(target_os = "macos") {
            "libbaml_cffi.dylib"
        } else if cfg!(target_os = "windows") {
            "baml_cffi.dll"
        } else {
            "libbaml_cffi.so"
        });

    Ok(dylib_path)
}

impl<L: TestLanguageFeatures> Drop for TestStructure<L> {
    fn drop(&mut self) {
        // delete src_dir if it exists
        if self.src_dir.exists() && !self.persist {
            let _ = std::fs::remove_dir_all(&self.src_dir);
        }
    }
}

impl<L: TestLanguageFeatures> TestStructure<L> {
    fn new(dir: PathBuf, generator: L, persist: bool) -> Result<Self, anyhow::Error> {
        let project_name = dir.iter().next_back().expect("must have a folder name");

        let cargo_root = get_cargo_root()?;
        let base_test_dir = cargo_root
            .join("generators/languages")
            .join(L::test_name())
            .join("generated_tests");
        let test_dir = utils::unique_dir(
            &base_test_dir,
            project_name.to_string_lossy().as_ref(),
            persist,
        );
        std::fs::create_dir_all(&test_dir)?;

        // clear test_dir only if it already exists (unlikely with unique_dir)
        let _ = std::fs::remove_dir_all(&test_dir);

        // copy language-specific sources + baml_src link
        let lang_dir = dir.join(L::test_name());
        if lang_dir.exists() {
            utils::copy_dir_flat(&lang_dir, &test_dir)?;
        }
        utils::create_symlink(&dir.join("baml_src"), &test_dir.join("baml_src"))?;

        let ir = make_test_ir_from_dir(&dir.join("baml_src"))?;

        Ok(Self {
            src_dir: test_dir,
            ir: std::sync::Arc::new(ir),
            generator,
            project_name: project_name.to_string_lossy().to_string(),
            persist,
        })
    }

    pub fn ensure_consistent_codegen(&self) -> Result<(), anyhow::Error> {
        // read all .baml_files in the test_dir
        let baml_files = glob::glob(self.src_dir.join("**/*.baml").to_str().unwrap())?;
        let baml_files = baml_files
            .into_iter()
            .map(|b| match b {
                Ok(b) => Ok((b.clone(), std::fs::read_to_string(b))),
                Err(e) => Err(e),
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(b, content)| match content {
                Ok(content) => Ok((
                    b.strip_prefix(&self.src_dir).unwrap().to_path_buf(),
                    content,
                )),
                Err(e) => Err(e),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let generate_files = |baml_files: &BTreeMap<PathBuf, String>| -> Result<_, anyhow::Error> {
            let client_type = baml_types::GeneratorOutputType::from_str(L::name())?;

            let args = GeneratorArgs {
                output_dir_relative_to_baml_src: self.src_dir.join("baml_client"),
                baml_src_dir: self.src_dir.join("baml_src"),
                inlined_file_map: baml_files.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                no_version_check: true,
                default_client_mode: baml_types::GeneratorDefaultClientMode::Async,
                on_generate: match L::test_name() {
                    "go" => {
                        vec![
                            format!(
                                "gofmt -w . && goimports -w . && go mod tidy && BAML_LIBRARY_PATH={} go test -run NEVER_MATCH",
                                get_dylib_path()?.display()
                            )
                            .to_string(),
                        ]
                    }
                    "python" => vec!["ruff check --fix".to_string()],
                    "typescript" => vec![],
                    "rust" => {
                        vec![
                            format!(
                                "rustfmt baml_client/*.rs 2>/dev/null || true && BAML_LIBRARY_PATH={} cargo test --no-run",
                                get_dylib_path()?.display()
                            )
                            .to_string(),
                        ]
                    }
                    // "ruby" => vec!["bundle install".to_string(), "srb init".to_string(), "srb tc --typed=strict".to_string()],
                    _ => vec![],
                },
                client_type,
                client_package_name: Some(self.project_name.clone()),
                module_format: None,
                is_pydantic_2: None,
            };
            let files = self
                .generator
                .generate_sdk_files_for_test(self.ir.clone(), &args)?;

            Ok(files)
        };

        // run 100 times and ensure the files are the same
        let generated_runs = (0..100)
            .map(|_| generate_files(&baml_files))
            .collect::<Result<Vec<_>, _>>()?;

        // ensure the files are the same
        for run in &generated_runs {
            assert_eq!(run, &generated_runs[0]);
        }

        Ok(())
    }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        let also_run_tests = std::env::var("RUN_GENERATOR_TESTS")
            .map(|v| v == "1")
            .unwrap_or(false);

        // read all .baml_files in the test_dir
        let baml_files = glob::glob(self.src_dir.join("**/*.baml").to_str().unwrap())?;
        let baml_files = baml_files
            .into_iter()
            .map(|b| match b {
                Ok(b) => Ok((b.clone(), std::fs::read_to_string(b))),
                Err(e) => Err(e),
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(b, content)| match content {
                Ok(content) => Ok((
                    b.strip_prefix(&self.src_dir).unwrap().to_path_buf(),
                    content,
                )),
                Err(e) => Err(e),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let client_type = baml_types::GeneratorOutputType::from_str(L::name())?;

        let args = GeneratorArgs {
            output_dir_relative_to_baml_src: self.src_dir.join("baml_client"),
            baml_src_dir: self.src_dir.join("baml_src"),
            inlined_file_map: baml_files,
            version: env!("CARGO_PKG_VERSION").to_string(),
            no_version_check: true,
            default_client_mode: baml_types::GeneratorDefaultClientMode::Async,
            on_generate: match L::test_name() {
                "go" => {
                    vec![
                        format!(
                            "gofmt -w . && goimports -w . && go mod tidy && BAML_LIBRARY_PATH={} go test -run NEVER_MATCH",
                            get_dylib_path()?.display()
                        )
                            .to_string(),
                    ]
                }
                "python" => vec!["ruff check --fix".to_string()],
                "typescript" => vec![],
                "rust" => {
                    vec![
                        format!(
                            "rustfmt baml_client/*.rs 2>/dev/null || true && BAML_LIBRARY_PATH={} RUSTFLAGS=-Awarnings cargo check",
                            get_dylib_path()?.display()
                        )
                        .to_string(),
                    ]
                }
                // "ruby" => vec!["bundle install".to_string(), "srb init".to_string(), "srb tc --typed=strict".to_string()],
                _ => vec![],
            },
            client_type,
            client_package_name: Some(self.project_name.clone()),
            module_format: None,
            is_pydantic_2: None,
        };
        self.generator.generate_sdk(self.ir.clone(), &args)?;

        for cmd_str in args.on_generate {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", &cmd_str]);
            cmd.current_dir(&self.src_dir);
            let output = cmd.output().expect("failed to run command");
            assert!(
                output.status.success(),
                "Failed to run command: {} (exit code: {}):\n{}\n{}",
                cmd_str,
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        if also_run_tests {
            let dylib_path = get_dylib_path()?;

            match args.client_type {
                baml_types::GeneratorOutputType::Go => {
                    let mut cmd = Command::new("go");
                    cmd.args(vec!["test", "-v"]);
                    cmd.current_dir(&self.src_dir);
                    cmd.env("BAML_LIBRARY_PATH", &dylib_path);
                    run_and_stream(&mut cmd)?;
                }
                baml_types::GeneratorOutputType::Rust => {
                    let mut cmd = Command::new("cargo");
                    cmd.args(vec!["test", "-v"]);
                    cmd.current_dir(&self.src_dir);
                    cmd.env(
                        "OPENAI_API_KEY",
                        std::env::var("OPENAI_API_KEY")
                            .unwrap_or_else(|_| "$OPENAI_API_KEY_NOT_SET".to_string()),
                    );
                    cmd.env("BAML_LIBRARY_PATH", &dylib_path);
                    cmd.env("RUSTFLAGS", "-Awarnings");
                    run_and_stream(&mut cmd)?;
                }
                _ => {
                    eprintln!(
                        "RUN_GENERATOR_TESTS=1 is set but test runner not implemented for {:?}",
                        args.client_type
                    );
                }
            }
        } else {
            eprintln!("Not running! Set RUN_GENERATOR_TESTS=1 to run tests");
        }

        Ok(())
    }
}

use std::{
    io::{BufRead, BufReader},
    process::Stdio,
    thread,
};

#[allow(clippy::print_stdout)]
fn run_and_stream(cmd: &mut Command) -> anyhow::Result<()> {
    // Pipe both streams before we spawn.
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    // Take ownership of the pipes.
    let stdout = child.stdout.take().expect("stdout pipe");
    let stderr = child.stderr.take().expect("stderr pipe");

    // Spawn two threads that forward each line to your library in real time.
    let out_handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            // Swap out the `println!` for whatever your library needs,
            // e.g. log::info! or a channel send.
            println!("{line}");
            let _ = std::io::stdout().flush();
        }
    });

    let err_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            eprintln!("{line}");
            // flush stderr
            let _ = std::io::stderr().flush();
        }
    });

    // Wait for the process *and* the threads.
    let status = child.wait()?;
    out_handle.join().unwrap();
    err_handle.join().unwrap();

    anyhow::ensure!(status.success(), "child exited with {}", status);
    Ok(())
}

pub struct TestHarness {}

impl TestHarness {
    pub fn load_test<L: TestLanguageFeatures>(
        name: &str,
        generator: L,
        persist: bool,
    ) -> Result<TestStructure<L>, anyhow::Error> {
        let cargo_root = get_cargo_root()?;
        let test_data_dir = cargo_root.join("generators/data").join(name);
        let test_structure = TestStructure::new(test_data_dir, generator, persist)?;
        Ok(test_structure)
    }
}

// Include the generated macro from build.rs
// this gives us: create_code_gen_test_suites!(LanguageFeatures)
include!(concat!(env!("OUT_DIR"), "/generated_macro.rs"));

mod utils {
    // util.rs (put near the top of the same file or in a new private module)

    use std::path::{Path, PathBuf};

    pub fn unique_dir(base: &Path, project: &str, persist: bool) -> PathBuf {
        if persist {
            return base.join(project);
        }

        base.join(format!(
            "{}_{}",
            project,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    pub fn create_symlink(src: &Path, dest: &Path) -> Result<(), anyhow::Error> {
        if dest.exists() {
            if dest.is_dir() && !dest.is_symlink() {
                std::fs::remove_dir_all(dest)?;
            } else {
                std::fs::remove_file(dest)?;
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            if symlink(src, dest).is_err() {
                fallback_copy(src, dest)?;
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::{symlink_dir, symlink_file};
            let md = std::fs::metadata(src)?;
            let res = if md.is_dir() {
                symlink_dir(src, dest)
            } else {
                symlink_file(src, dest)
            };
            if res.is_err() {
                fallback_copy(src, dest)?;
            }
        }

        Ok(())
    }

    fn fallback_copy(src: &Path, dest: &Path) -> Result<(), anyhow::Error> {
        if src.is_dir() {
            std::fs::create_dir_all(dest)?;
            for e in std::fs::read_dir(src)? {
                let e = e?;
                create_symlink(&e.path(), &dest.join(e.file_name()))?;
            }
        } else {
            std::fs::copy(src, dest)?;
        }
        Ok(())
    }

    pub fn copy_dir_flat(src: &Path, dest: &Path) -> Result<(), anyhow::Error> {
        if dest.exists() {
            std::fs::remove_dir_all(dest).ok();
        }
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            create_symlink(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    }
}
