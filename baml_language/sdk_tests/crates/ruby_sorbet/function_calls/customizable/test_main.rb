# frozen_string_literal: true
require "minitest/autorun"
require "open3"
require "rbconfig"
require "baml_sdk"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MainTest < Minitest::Test
  # SDK_PARITY_LINT(skip): Ruby package initialization on the first call in a fresh process
  def test_hello_world_automatically_initializes
    # A fresh process proves this is the first call, regardless of test order.
    assert_fresh_process('raise unless BamlSdk.hello_world == "hello world"')
  end

  # SDK_PARITY_LINT(skip): Ruby package explicit initialization in a fresh process
  def test_explicit_initialization_then_call
    assert_fresh_process('BamlSdk.initialize!; raise unless BamlSdk.hello_world == "hello world"')
  end

  def test_main_hello_world_returns_literal
    assert_equal 0, BamlSdk.method(:hello_world).arity
    assert_equal "hello world", BamlSdk.hello_world
  end

  def test_main_single_required_arg_round_trips
    assert_equal 1, BamlSdk.method(:single_required_arg).arity
    assert_equal "hi", BamlSdk.single_required_arg("hi")
    assert_raises(TypeError) { BamlSdk.single_required_arg(1) }
  end

  # SDK_PARITY_LINT(skip): Ruby primitive encoding and decoding smoke in the function_calls fixture
  def test_primitive_round_trips
    [0, -42, (2**62) - 1, -(2**62)].each do |value|
      assert_equal value, BamlSdk.round_trip_int(value)
    end
    [true, false].each { |value| assert_equal value, BamlSdk.round_trip_bool(value) }
    ["", "héllo\u0000world"].each { |value| assert_equal value, BamlSdk.round_trip_string(value) }
    assert_equal 1.25, BamlSdk.round_trip_float(1.25)
    assert_nil BamlSdk::ThrowsTest.sleep_ms(0)
  end

  # SDK_PARITY_LINT(skip): Ruby generated T::Struct round trip in the function_calls fixture
  def test_person_round_trip
    person = BamlSdk::Person.new(person: "person", name: "Ryan", age: 30)
    result = BamlSdk.round_trip_person(person)
    assert_instance_of BamlSdk::Person, result
    refute_same person, result
    assert_equal person.person, result.person
    assert_equal person.name, result.name
    assert_equal person.age, result.age
  end

  private

  def assert_fresh_process(code)
    stdout, stderr, status = Open3.capture3(
      RbConfig.ruby, "-I", $LOAD_PATH.join(File::PATH_SEPARATOR), "-rbaml_sdk", "-e", code
    )
    assert status.success?, "#{stdout}\n#{stderr}"
  end
end
