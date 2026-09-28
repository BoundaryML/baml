#[cfg(test)]
sdk_test_harness_runner::setup_guard!("SDK_TEST_RUBY_SORBET_SETUP");

#[cfg(test)]
mod bridge_tests {
    #[test]
    fn generated_packages_load() {
        for fixture in ["function_calls", "llm_functions"] {
            sdk_test_harness_runner::run_workspace_cmd(
                "sdk_tests/crates/ruby_sorbet",
                &format!(
                    "ruby -S bundle exec ruby -I ../../../sdks/ruby/bridge_ruby/lib -I {fixture}/generated {fixture}/generated/generated_package_test.rb"
                ),
                "ruby-bundle",
                "BUNDLE_PATH",
            );
        }
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
