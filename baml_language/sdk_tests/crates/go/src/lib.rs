//! Go sdk-test crate. One `go test ./...` per fixture, declared below and
//! expanded by `sdk_test_harness_runner::go::test_suite!`.
//!
//! The `fixture` rows must match `sdk_test_harness_runner::fixtures::SHARED`;
//! the generated `fixture_manifest::matches_corpus` test enforces it.
#[cfg(test)]
sdk_test_harness_runner::go::test_suite! {
    fixture docstrings_etc;
    fixture function_calls;
    fixture llm_functions;
    fixture type_shapes;
    fixture unsupported_only;
    // No `baml_src`: built from a hand-constructed SymbolPool to exercise
    // cross-package import collisions BAML source cannot express.
    synthetic package_edges;
}

#[cfg(test)]
mod generated_formatting_tests {
    use std::{fs, path::Path, process::Command};

    #[test]
    fn every_generator_owned_go_file_is_gofmt_clean() {
        // Sourced from the suite's own list rather than repeated: a fixture
        // named here but never generated would contribute zero files and slip
        // past the `checked > 0` floor below.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;

        for fixture in crate::FIXTURES {
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
        let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("missing-generated-sdk");
        assert!(go_files_below(&missing).is_empty());
    }
}
