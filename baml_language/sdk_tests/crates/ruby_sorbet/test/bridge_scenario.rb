# frozen_string_literal: true

require "timeout"
$LOAD_PATH.unshift File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
require "baml/bridge"

ENV["BAML_FAKE_RUNTIME_VERSION"] = Baml::Bridge::VERSION
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_FIXTURE")
ENV.delete("BAML_FAKE_NATIVE_MODE")
Baml::Bridge.register_program("valid-bytecode".b)

def assert_raises(error_class)
  begin
    yield
  rescue error_class => error
    return error
  end
  raise "expected #{error_class}"
end

case ARGV.fetch(0)
when "truncated", "null_function"
  ENV["BAML_FAKE_NATIVE_MODE"] = ARGV[0]
  error = assert_raises(Baml::Bridge::IncompatibleRuntimeError) { Baml::Bridge.initialize! }
  expected = ARGV[0] == "truncated" ? "BamlApiV1 is truncated" : "BamlApiV1.media_from_file_content is null"
  raise error.message unless error.message.include?(expected)
when "pid_check"
  exit 0 unless Process.respond_to?(:fork)

  Baml::Bridge.initialize!
  child = fork do
    assert_raises(Baml::Bridge::ForkSafetyError) { Baml::Bridge.initialize! }
    exit! 0
  end
  begin
    status = Timeout.timeout(10) { Process.wait2(child).last }
    raise "forked child failed" unless status.success?
  rescue Timeout::Error
    Process.kill("KILL", child)
    Process.wait(child)
    raise
  end
when "different_program"
  Baml::Bridge.initialize!
  assert_raises(Baml::Bridge::ProgramConflictError) { Baml::Bridge.register_program("different") }
when "abandoned_call"
  assert_raises(TypeError) { Baml::Bridge.call_async("user.test", { "x" => Object.new }) }
  library = FFI::DynamicLibrary.open(ENV.fetch("BAML_RUBY_TEST_FIXTURE"), FFI::DynamicLibrary::RTLD_NOW)
  count = FFI::Function.new(:uint32, [], library.find_function("baml_test_release_count")).call
  raise "abandoned call released #{count} times" unless count == 1
else
  raise "unknown scenario #{ARGV.first.inspect}"
end
