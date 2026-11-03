# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MainTest < Minitest::Test
  def test_main_hello_world_returns_literal
    assert_raises(ArgumentError) { BamlSdk.hello_world("extra") }
    assert_equal "hello world", BamlSdk.hello_world
  end

  def test_main_single_required_arg_round_trips
    assert_raises(ArgumentError) { BamlSdk.single_required_arg }
    assert_equal "hi", BamlSdk.single_required_arg("hi")
    assert_raises(TypeError) { BamlSdk.single_required_arg(1) }
  end

end
