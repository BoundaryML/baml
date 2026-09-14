# frozen_string_literal: true

require "minitest/autorun"
require "open3"
require "rbconfig"
require_relative "../../../../sdks/ruby/bridge_ruby/lib/baml/bridge"

class BridgeLoaderTest < Minitest::Test
  REQUIRED_FUNCTION_FIELDS = Baml::Bridge
                             .const_get(:Native, false)
                             .const_get(:ApiV1, false)
                             .const_get(:FUNCTION_FIELDS, false)
                             .map(&:to_s)
                             .freeze

  TERMINAL_MODES = %w[
    null-table
    wrong-abi
    truncated
    version-mismatch
    invalid-version-utf8
    null-version-pointer
    registration-rejected
  ].freeze

  def test_configuration_and_open_failures_can_retry
    run_scenario("configuration_retry")
    run_scenario("open_retry")
    run_scenario("concurrent_open_failure_preserves_claim")
  end

  def test_opened_incompatible_libraries_are_terminal
    TERMINAL_MODES.each do |mode|
      run_scenario("terminal_mode", mode)
    end
    run_scenario("terminal_missing_getter")
  end

  def test_every_required_v1_function_is_validated
    REQUIRED_FUNCTION_FIELDS.each do |field|
      run_scenario("terminal_null_field", field)
    end
  end

  def test_invalid_bytecode_can_retry_and_ready_program_uses_exact_bytes
    run_scenario("invalid_bytecode_retry_and_identity")
  end

  def test_concurrent_initialization_calls_native_once
    run_scenario("concurrent_initialization")
  end

  def test_fork_policy
    run_scenario("fork_before_native_use")
    run_scenario("fork_after_native_use")
    run_scenario("fork_during_initialization")
    run_scenario("fork_during_open_failure")
  end

  def test_process_runtime_retains_registered_callback_across_gc
    run_scenario("registered_callback_retention")
  end

  def test_foreign_native_thread_callback_copies_bytes_and_contains_exceptions
    run_scenario("foreign_thread_callback")
  end

  def test_real_bridge_initializes_real_bytecode
    run_scenario("real_runtime_initialization")
  end

  def test_initialize_requires_byte_string
    run_scenario("type_validation")
  end

  private

  def run_scenario(name, *arguments)
    scenario = File.expand_path("bridge_scenario.rb", __dir__)
    stdout, stderr, status = Open3.capture3(
      RbConfig.ruby,
      scenario,
      name,
      *arguments
    )
    assert status.success?, <<~MESSAGE
      Ruby bridge scenario #{([name] + arguments).join(" ")} failed
      stdout:
      #{stdout}
      stderr:
      #{stderr}
    MESSAGE
  end
end
