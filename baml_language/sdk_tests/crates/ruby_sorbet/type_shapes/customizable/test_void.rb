# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class VoidTest < Minitest::Test
  def test_void_no_op
    assert_nil BamlSdk::Void.no_op()
  end

end
