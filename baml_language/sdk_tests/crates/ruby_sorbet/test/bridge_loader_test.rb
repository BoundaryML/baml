# frozen_string_literal: true

require "minitest/autorun"
require "open3"
require "rbconfig"
require "timeout"
require "fileutils"
require "tmpdir"
$LOAD_PATH.unshift File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
require "baml/bridge"

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

  def test_fork_during_native_open_rejects_before_touching_the_inherited_mutex
    skip "fork is unavailable" unless Process.respond_to?(:fork)

    runtime = Baml::Bridge.const_get(:ProcessRuntime, false).new
    runtime.register_program("bytecode")
    runtime.define_singleton_method(:configured_runtime_path!) { "fake-runtime" }
    runtime.define_singleton_method(:open_library) do |*|
      child = fork do
        begin
          Timeout.timeout(2) { runtime.initialize! }
          exit! 1
        rescue Baml::Bridge::ForkSafetyError
          exit! 0
        end
      end
      _, status = Process.wait2(child)
      raise "child touched inherited loader state" unless status.success?
      raise LoadError, "test open failed"
    end
    assert_raises(Baml::Bridge::RuntimeLoadError) { runtime.initialize! }
    assert_equal false, runtime.instance_variable_get(:@native_load_in_progress)

    # A completed failed open left no native state, so a child may retry.
    runtime.define_singleton_method(:open_library) { |*| raise LoadError, "retry reached open" }
    child = fork do
      begin
        runtime.initialize!
        exit! 1
      rescue Baml::Bridge::RuntimeLoadError => error
        exit!(error.message.include?("retry reached open") ? 0 : 1)
      end
    end
    _, status = Process.wait2(child)
    assert status.success?, "failed native open must permit a child retry"
  end

  def test_cache_targets_match_the_shared_bridge_layout
    targets = [
      ["x86_64", "linux", "x86_64-linux", "x86_64-unknown-linux-gnu", "libbridge_cffi.so"],
      ["aarch64", "linux", "aarch64-linux-musl", "aarch64-unknown-linux-musl", "libbridge_cffi.so"],
      ["x86_64", "linux-musl", "x86_64-linux", "x86_64-unknown-linux-musl", "libbridge_cffi.so"],
      ["aarch64", "linux", "aarch64-linux", "aarch64-unknown-linux-gnu", "libbridge_cffi.so"],
      ["x86_64", "darwin25", "x86_64-darwin25", "x86_64-apple-darwin", "libbridge_cffi.dylib"],
      ["arm64", "darwin25", "arm64-darwin25", "aarch64-apple-darwin", "libbridge_cffi.dylib"],
      ["x64", "mingw32", "x64-mingw-ucrt", "x86_64-pc-windows-msvc", "bridge_cffi.dll"],
      ["arm64", "mswin64", "arm64-mswin64", "aarch64-pc-windows-msvc", "bridge_cffi.dll"]
    ]
    with_cache do |runtime, home|
      targets.each do |cpu, os, arch, target, filename|
        RbConfig::CONFIG.stub(:values_at, [cpu, os, arch]) do
          assert_equal File.join(home, "bridges", Baml::Bridge::TOOLCHAIN_VERSION, target, filename),
                       runtime.send(:cached_runtime_path).to_s
        end
      end
    end
  end

  private

  def with_cache
    original = ENV.to_h.slice("BAML_HOME", "BAML_BRIDGE_PATH")
    Dir.mktmpdir("baml-loader-") do |home|
      ENV["BAML_HOME"] = home
      ENV.delete("BAML_BRIDGE_PATH")
      yield Baml::Bridge.const_get(:ProcessRuntime, false).new, home
    end
  ensure
    %w[BAML_HOME BAML_BRIDGE_PATH].each { |key| ENV[key] = original[key] }
  end

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
