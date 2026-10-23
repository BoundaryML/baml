# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class PrimitivesTest < Minitest::Test
  def test_primitives_return_int
    assert_equal 42, BamlSdk::Primitives.return_int()
  end

end
