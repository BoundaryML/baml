#[cfg(test)]
sdk_test_harness_runner::setup_guard!("SDK_TEST_RUBY_SORBET_SETUP");

#[cfg(test)]
mod class_loading;

#[cfg(test)]
mod bridge_tests {
    fn generated_package_test(fixture: &str) {
        let generated = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(fixture)
            .join("generated");
        let mut tests: Vec<_> = std::fs::read_dir(&generated)
            .expect("generated fixture directory")
            .map(|entry| entry.expect("generated fixture entry").file_name())
            .filter(|name| {
                let name = name.to_string_lossy();
                name.starts_with("test_") && name.ends_with(".rb")
            })
            .collect();
        tests.sort();
        assert!(
            !tests.is_empty(),
            "no Ruby tests in {}",
            generated.display()
        );
        for test in tests {
            let test = test.to_string_lossy();
            sdk_test_harness_runner::run_workspace_cmd(
                "sdk_tests/crates/ruby_sorbet",
                &format!(
                    "ruby -S bundle exec ruby -I ../../../sdks/ruby/bridge_ruby/lib -I {fixture}/generated {fixture}/generated/{test}"
                ),
                "ruby-bundle",
                "BUNDLE_PATH",
            );
        }
    }

    #[test]
    fn function_calls() {
        generated_package_test("function_calls");
    }

    #[test]
    fn llm_functions() {
        generated_package_test("llm_functions");
    }

    #[test]
    #[cfg_attr(
        not(all(
            any(target_arch = "x86_64", target_arch = "aarch64"),
            any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))
        )),
        ignore = "Sorbet 0.6.13506 binaries require x86_64/arm64 macOS or x86_64/aarch64 glibc Linux; runtime tests still run"
    )]
    fn static_consumer() {
        let fixture = super::class_loading::generate();
        let output = sdk_test_harness_runner::run_command(
            std::process::Command::new("ruby")
                .arg("test/static/consumer_gate.rb")
                .arg(fixture.path())
                .current_dir(env!("CARGO_MANIFEST_DIR")),
            "pinned Sorbet consumer gate",
        );
        sdk_test_harness_runner::assert_stdout_contains(
            &output,
            "Sorbet consumer gate: 3 clean baselines and 4 exact negative diagnostics",
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    #[test]
    fn recursive_class_loading() {
        let fixture = super::class_loading::generate();
        let output = sdk_test_harness_runner::run_command(
            std::process::Command::new("ruby")
                .args([
                    "-S",
                    "bundle",
                    "exec",
                    "ruby",
                    "-I",
                    "../../../sdks/ruby/bridge_ruby/lib",
                    "-I",
                ])
                .arg(fixture.path())
                .arg("test/class_loading_probe.rb")
                .current_dir(env!("CARGO_MANIFEST_DIR")),
            "generated recursive class loading",
        );
        sdk_test_harness_runner::assert_stdout_contains(&output, "1 runs,");
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }

    #[test]
    fn protocol_values() {
        sdk_test_harness_runner::run_workspace_cmd(
            "sdk_tests/crates/ruby_sorbet",
            "ruby -S bundle exec ruby test/protocol_test.rb",
            "ruby-bundle",
            "BUNDLE_PATH",
        );
    }

    #[test]
    fn synchronous_call_runtime() {
        sdk_test_harness_runner::run_workspace_cmd(
            "sdk_tests/crates/ruby_sorbet",
            "ruby -S bundle exec ruby test/bridge_call_test.rb",
            "ruby-bundle",
            "BUNDLE_PATH",
        );
    }

    #[test]
    fn loader_and_lifecycle() {
        sdk_test_harness_runner::run_workspace_cmd(
            "sdk_tests/crates/ruby_sorbet",
            "ruby -S bundle exec ruby test/bridge_loader_test.rb",
            "ruby-bundle",
            "BUNDLE_PATH",
        );
    }
}
