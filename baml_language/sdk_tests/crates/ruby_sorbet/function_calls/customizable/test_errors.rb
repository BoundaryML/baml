# frozen_string_literal: true
require "minitest/autorun"
require "open3"
require "rbconfig"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ErrorsTest < Minitest::Test
  def test_errors_user_panic_surfaces_as_baml_panic
    error = assert_raises(Baml::Panic) { BamlSdk::ThrowsTest.do_panic("user-initiated boom") }
    assert_instance_of BamlSdk::Baml::Panics::UserPanic, error.value
    assert_equal "baml.panics.UserPanic", error.type_name
    assert_equal "user-initiated boom", error.message
    refute_kind_of StandardError, error
  end

  def test_errors_union_throws_preserves_class_name
    single = assert_raises(Baml::Error) { BamlSdk::RaisesTest.reparse("x") }
    union = assert_raises(Baml::Error) { BamlSdk::RaisesTest.load_doc("x") }
    assert_equal "user.raises_test.ParseError", single.type_name
    assert_equal single.type_name, union.type_name
    assert_instance_of BamlSdk::RaisesTest::ParseError, union.value
    assert_equal "x", single.message
    assert_equal single.message, union.message
  end

  def test_errors_stdlib_error_surfaces_as_baml_error
    error = assert_raises(Baml::Error) { BamlSdk::ThrowsTest.parse_json("{not valid json") }
    assert_instance_of BamlSdk::Baml::Json::ParseError, error.value
  end

  def test_errors_user_throw_surfaces_declared_instance
    error = assert_raises(Baml::Error) { BamlSdk::ThrowsTest.throw_my_error }
    assert_instance_of BamlSdk::ThrowsTest::MyError, error.value
  end

  def test_errors_host_invalid_argument_wraps_baml_errors_invalid_argument
    # Ruby binds generated arguments before calling BAML. An undeclared
    # keyword raises ArgumentError, rather than a synthesized BAML payload.
    assert_raises(ArgumentError) { BamlSdk.hello_world(not_a_param: 2) }
    error = assert_raises(Baml::Error) do
      Baml::Bridge.call_async("user.hello_world", { "not_a_param" => 2 }).wait
    end
    assert_instance_of BamlSdk::Baml::Errors::InvalidArgument, error.value
  end

  def test_errors_cancellation_surfaces_as_baml_panic
    token = BamlSdk::Baml::Spawn::CancelToken.baml_new
    call = BamlSdk::ThrowsTest.sleep_ms_async(60_000, _baml: Baml::Options.new(cancel: token))
    timer = Thread.new { sleep 0.1; token.cancel }
    error = assert_raises(Baml::CancelledError) { call.wait(timeout: 5) }
    assert_kind_of Baml::Panic, error
    refute_kind_of StandardError, error
    assert_instance_of BamlSdk::Baml::Panics::Cancelled, error.value
    timer.join
  ensure
    timer&.kill&.join
  end

  def test_errors_str_is_non_empty
    error = assert_raises(Baml::Error) { BamlSdk::ThrowsTest.parse_json("{not valid json") }
    refute_empty error.to_s
  end

  def test_errors_baml_error_carries_baml_trace
    error = assert_raises(Baml::Error) { BamlSdk::ThrowsTest.throw_my_error }
    assert_instance_of Array, error.baml_trace
    refute_empty error.baml_trace
    frame = /\AFile "([^"]*)", line (\d+), in (.*)\z/.match(error.baml_trace.last)
    refute_nil frame
    assert frame[1].end_with?("types.baml"), frame[1]
    assert_operator frame[2].to_i, :>=, 1
    assert_equal "user.throws_test.ThrowMyError", frame[3]
  end

  def test_errors_baml_trace_spliced_into_python_traceback
    # Like Java's port, assert Ruby's native rendering includes BAML frames.
    error = assert_raises(Baml::Error) { BamlSdk::ThrowsTest.parse_json("{not valid json") }
    refute_empty error.baml_trace
    rendered = error.full_message
    assert_includes rendered, "types.baml"
    assert_includes rendered, "user.throws_test.ParseJson"
  end

  def test_errors_clean_exit_terminates_process_with_code_0
    assert_exit_code(0)
  end

  def test_errors_clean_exit_terminates_process_with_code_7
    assert_exit_code(7)
  end

  # SDK_PARITY_LINT(skip): Ruby error wrapper for a thrown string
  def test_primitive_thrown_error
    error = assert_raises(Baml::Error) { BamlSdk.unhandled_spawn_error }
    assert_equal "string", error.type_name
    assert_equal "boom", error.message
    assert_equal "boom", error.value
  end

  private

  def assert_exit_code(code)
    stdout, stderr, status = Open3.capture3(
      RbConfig.ruby, "-I", $LOAD_PATH.join(File::PATH_SEPARATOR), "-rbaml_sdk", "-e",
      "at_exit { puts 'UNREACHABLE' }; BamlSdk::ThrowsTest.do_exit(#{code}); puts 'UNREACHABLE'"
    )
    assert_equal code, status.exitstatus, "#{stdout}\n#{stderr}"
    refute_includes stdout, "UNREACHABLE"
  end
end
