#[cfg(test)]
sdk_test_harness_runner::ruby_sorbet::test_suite! {
    fixture docstrings_etc { minitest: later, sorbet: later }
    fixture function_calls { minitest: on, sorbet: on }
    fixture llm_functions { minitest: on, sorbet: on }
    fixture type_shapes { minitest: on, sorbet: on }
    fixture unsupported_only { minitest: later, sorbet: later }
}

#[cfg(test)]
mod wrong_argument {
    sdk_test_harness_runner::__ruby_sorbet_gate!(on, sorbet, {
        let mut command = sdk_test_harness_runner::ruby_sorbet::sorbet_command("function_calls");
        command.arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test/static/wrong_argument.rb"),
        );
        let output = command.output().expect("run Sorbet negative consumer");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(100), "{diagnostics}");
        assert_eq!(
            diagnostics.matches("https://srb.help/").count(),
            1,
            "{diagnostics}"
        );
        assert!(
            diagnostics.contains("wrong_argument.rb:4:"),
            "{diagnostics}"
        );
        assert!(
            diagnostics.contains("https://srb.help/7002"),
            "{diagnostics}"
        );
    });
}

#[cfg(test)]
mod bridge_tests {
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
