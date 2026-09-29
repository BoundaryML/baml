# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
require_relative "replay_harness"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ReplayHarnessTest < Minitest::Test
  # SDK_PARITY_LINT(skip): Ruby replay harness removes environment overrides after success
  def test_replay_server_cleans_up_after_success
    ReplayHarness.with_server("replay_extract_doc") do |addr|
      assert_equal "http://#{addr}", ENV["BAML_REPLAY_BASE_URL"]
      assert_equal "replay-test-key", ENV["BAML_REPLAY_API_KEY"]
    end
    assert_nil ENV["BAML_REPLAY_BASE_URL"]
    assert_nil ENV["BAML_REPLAY_API_KEY"]
  end

  # SDK_PARITY_LINT(skip): Ruby replay harness propagates thread failures and removes temporary state
  def test_replay_server_failure_surfaces_and_cleans_up
    addr_file = nil
    failure = RuntimeError.new("server startup failed")
    serve = lambda do |_recording, path|
      addr_file = path
      raise failure
    end
    BamlSdk::Replay.stub(:replay_serve_until_shutdown, serve) do
      error = Timeout.timeout(2) do
        assert_raises(RuntimeError) do
          ReplayHarness.with_server("replay_extract_doc") { flunk "server should not start" }
        end
      end
      assert_same failure, error
    end
    refute File.exist?(addr_file)
    assert_nil ENV["BAML_REPLAY_BASE_URL"]
    assert_nil ENV["BAML_REPLAY_API_KEY"]
  end
end
