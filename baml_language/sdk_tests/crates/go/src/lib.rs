#[cfg(test)]
sdk_test_harness_runner::go::test_suite!();

#[cfg(test)]
mod generated_formatting_tests {
    use std::{fs, path::Path, process::Command};

    /// This crate's source dir, where setup writes the generated Go trees.
    /// The compile-time `CARGO_MANIFEST_DIR` is baked into the test binary,
    /// and for the CI nix unit graph that is a build sandbox
    /// (`/build/sdk_test_go-<ver>/`) that does not exist when the prebuilt
    /// binary runs - the gofmt sweep below would then find zero files and
    /// fail its own `checked > 0` guard. `BAML_SDK_TEST_GO_DIR` binds the
    /// dir when set; unset, behavior is byte-identical. Same pattern as
    /// `BAML_SURFACE_SNAPSHOT_DIR` and `BAML_PARAM_SCHEMA_GOLDEN`, which
    /// exist for the same relocated-build reason.
    fn crate_dir() -> std::path::PathBuf {
        std::env::var_os("BAML_SDK_TEST_GO_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    #[test]
    fn every_generator_owned_go_file_is_gofmt_clean() {
        let fixtures = [
            "docstrings_etc",
            "function_calls",
            "host_reflect",
            "llm_functions",
            "package_edges",
            "type_shapes",
            "unsupported_only",
        ];
        let manifest_dir = crate_dir();
        let mut checked = 0;

        for fixture in fixtures {
            let generated_sdk = manifest_dir.join(fixture).join("generated/baml_sdk");
            for path in go_files_below(&generated_sdk) {
                let output = Command::new("gofmt")
                    .arg("-d")
                    .arg(&path)
                    .output()
                    .unwrap_or_else(|error| {
                        panic!("failed to run gofmt for {}: {error}", path.display())
                    });
                assert!(
                    output.status.success(),
                    "gofmt failed for {}:\n{}",
                    path.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    output.stdout.is_empty(),
                    "generator-owned file {} is not gofmt-clean:\n{}",
                    path.display(),
                    String::from_utf8_lossy(&output.stdout)
                );
                checked += 1;
            }
        }

        assert!(
            checked > 0,
            "formatting regression did not find generated Go files"
        );
    }

    fn go_files_below(root: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        collect_go_files(root, &mut files);
        files.sort();
        files
    }

    fn collect_go_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("failed to read {}: {error}", directory.display()),
        };
        for entry in entries {
            let path = entry.expect("generated directory entry").path();
            if path.is_dir() {
                collect_go_files(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "go") {
                files.push(path);
            }
        }
    }

    #[test]
    fn missing_generated_sdk_is_an_empty_fixture() {
        let missing = crate_dir().join("missing-generated-sdk");
        assert!(go_files_below(&missing).is_empty());
    }
}
