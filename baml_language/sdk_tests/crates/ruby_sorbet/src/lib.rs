#[cfg(test)]
sdk_test_harness_runner::setup_guard!("SDK_TEST_RUBY_SORBET_SETUP");

#[cfg(test)]
mod bridge_tests {
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
