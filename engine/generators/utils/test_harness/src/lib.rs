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
}

fn get_cargo_root() -> Result<PathBuf, anyhow::Error> {
    let cargo_root = std::env::var("CARGO_MANIFEST_DIR")?;
    Ok(PathBuf::from(cargo_root).join("../../..").canonicalize()?)
}

impl<L: TestLanguageFeatures> TestStructure<L> {
    fn new(dir: PathBuf, generator: L) -> Result<Self, anyhow::Error> {
        let project_name = dir.iter().last().expect("must have a folder name");
        // Copy the dir to cargo_root/generators/languages/{generator::name}/tests/{dir_name}
        let cargo_root = get_cargo_root()?;
        let test_dir = cargo_root
            .join("generators/languages")
            .join(L::test_name())
            .join("generated_tests")
            .join(project_name);

        fn create_symlink(src: &PathBuf, dest: &PathBuf) -> Result<(), anyhow::Error> {
            #[cfg(unix)]
            std::os::unix::fs::symlink(src, dest)?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(src, dest)?;

            Ok(())
        }

        fn copy_dir_recursive(src: &PathBuf, dest: &PathBuf) -> Result<(), anyhow::Error> {
            std::fs::create_dir_all(dest)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let dest_path = dest.join(entry.file_name());
                create_symlink(&entry.path(), &dest_path)?;
            }
            Ok(())
        }

        // clear test_dir
        let _ = std::fs::remove_dir_all(&test_dir);
        copy_dir_recursive(&dir.join(L::test_name()), &test_dir)?;
        create_symlink(&dir.join("baml_src"), &test_dir.join("baml_src"))?;

        let ir = make_test_ir_from_dir(&dir.join("baml_src"))?;

        Ok(Self {
            src_dir: test_dir,
            ir: std::sync::Arc::new(ir),
            generator,
            project_name: project_name.to_string_lossy().to_string(),
        })
    }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        let also_run_tests = std::env::var("RUN_GENERATOR_TESTS")
            .map(|v| v == "1")
            .unwrap_or(false);

        // read all .baml_files in the test_dir
        let baml_files = glob::glob(&self.src_dir.join("**/*.baml").to_str().unwrap())?;
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
                "go" => vec!["gofmt -w . && goimports -w . && go build".to_string()],
                "python" => vec!["ruff check --fix".to_string()],
                "typescript" => vec!["npx prettier --write .".to_string()],
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
            cmd.args(&["-c", &cmd_str]);
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
            match args.client_type {
                baml_types::GeneratorOutputType::Go => {
                    let mut cmd = Command::new(&format!("./{}", &self.project_name));
                    cmd.current_dir(&self.src_dir);
                    let dylib_path = get_cargo_root()?.join("target/debug/libbaml_cffi.dylib");
                    let so_path = get_cargo_root()?.join("target/debug/libbaml_cffi.so");
                    let cargo_target_dir = if dylib_path.exists() {
                        dylib_path
                    } else {
                        so_path
                    };
                    cmd.env("BAML_LIBRARY_PATH", cargo_target_dir);
                    run_and_stream(&mut cmd)?;
                }
                _ => {}
            }
        } else {
            println!("Not running! Set RUN_GENERATOR_TESTS=1 to run tests");
        }

        Ok(())
    }
}

use std::{
    io::{BufRead, BufReader},
    process::Stdio,
    thread,
};

fn run_and_stream(cmd: &mut Command) -> anyhow::Result<()> {
    // Pipe both streams before we spawn.
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

    // Take ownership of the pipes.
    let stdout = child.stdout.take().expect("stdout pipe");
    let stderr = child.stderr.take().expect("stderr pipe");

    // Spawn two threads that forward each line to your library in real time.
    let out_handle = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            // Swap out the `println!` for whatever your library needs,
            // e.g. log::info! or a channel send.
            println!("{line}");
            let _ = std::io::stdout().flush();
        }
    });

    let err_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
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
    ) -> Result<TestStructure<L>, anyhow::Error> {
        let cargo_root = get_cargo_root()?;
        let test_data_dir = cargo_root.join("generators/data").join(name);
        let test_structure = TestStructure::new(test_data_dir, generator)?;
        Ok(test_structure)
    }
}

// Include the generated macro from build.rs
// this gives us: create_code_gen_test_suites!(LanguageFeatures)
include!(concat!(env!("OUT_DIR"), "/generated_macro.rs"));
