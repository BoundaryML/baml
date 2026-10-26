# frozen_string_literal: true

require "minitest/autorun"
require "open3"
require "rbconfig"

class BridgeLoaderTest < Minitest::Test
  def test_truncated_table
    run_scenario("truncated")
  end

  def test_null_function_slot
    run_scenario("null_function")
  end

  def test_pid_check
    run_scenario("pid_check")
  end

  def test_different_program
    run_scenario("different_program")
  end

  def test_abandoned_call_is_released
    run_scenario("abandoned_call")
  end

  private

  def run_scenario(name)
    scenario = File.expand_path("bridge_scenario.rb", __dir__)
    stdout, stderr, status = Open3.capture3(RbConfig.ruby, scenario, name)
    assert status.success?, <<~MESSAGE
      Ruby bridge scenario #{name} failed
      stdout:
      #{stdout}
      stderr:
      #{stderr}
    MESSAGE
  end
end
