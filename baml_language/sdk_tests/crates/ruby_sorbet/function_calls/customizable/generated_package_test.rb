# frozen_string_literal: true
require "minitest/autorun"
require "open3"
require "rbconfig"
require "baml_sdk"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class GeneratedPackageTest < Minitest::Test
  def test_hello_world_automatically_initializes
    # A fresh process proves this is the first call, regardless of test order.
    assert_fresh_process('raise unless BamlSdk.hello_world == "hello world"')
  end

  def test_explicit_initialization_then_call
    assert_fresh_process('BamlSdk.initialize!; raise unless BamlSdk.hello_world == "hello world"')
  end

  def test_free_functions
    assert_equal 0, BamlSdk.method(:hello_world).arity
    assert_equal 1, BamlSdk.method(:single_required_arg).arity
    assert_equal "hello world", BamlSdk.hello_world
    assert_equal "hi", BamlSdk.single_required_arg("hi")
    assert_raises(TypeError) { BamlSdk.single_required_arg(1) }
  end

  def test_primitive_round_trips
    [0, -42, (2**62) - 1, -(2**62)].each do |value|
      assert_equal value, BamlSdk.round_trip_int(value)
    end
    [true, false].each { |value| assert_equal value, BamlSdk.round_trip_bool(value) }
    ["", "héllo\u0000world"].each { |value| assert_equal value, BamlSdk.round_trip_string(value) }
    assert_equal 1.25, BamlSdk.round_trip_float(1.25)
    assert_nil BamlSdk::ThrowsTest.sleep_ms(0)
  end

  def test_panic
    error = assert_raises(Baml::PanicError) { BamlSdk::ThrowsTest.do_panic("panic from Ruby") }
    assert_equal "baml.panics.UserPanic", error.type_name
    assert_equal "panic from Ruby", error.message
    refute_kind_of Baml::Error, error
  end

  def test_primitive_thrown_error
    error = assert_raises(Baml::Error) { BamlSdk.unhandled_spawn_error }
    assert_equal "string", error.type_name
    assert_equal "boom", error.message
  end

  def test_class_error_identity_without_decoding_the_class
    error = assert_raises(Baml::Error) { BamlSdk::RaisesTest.reparse("invalid document") }
    assert_equal "user.raises_test.ParseError", error.type_name
    assert_equal "invalid document", error.message
  end

  def test_unsupported_class_argument
    person = BamlSdk::Person.new(person: "person", name: "Ryan", age: 30)
    error = assert_raises(Baml::Bridge::UnsupportedTypeError) { BamlSdk.round_trip_person(person) }
    assert_includes error.message, "BamlSdk::Person"
    assert_includes error.message, "not yet supported"
  end

  private

  def assert_fresh_process(code)
    stdout, stderr, status = Open3.capture3(
      RbConfig.ruby, "-I", $LOAD_PATH.join(File::PATH_SEPARATOR), "-rbaml_sdk", "-e", code
    )
    assert status.success?, "#{stdout}\n#{stderr}"
  end
end
