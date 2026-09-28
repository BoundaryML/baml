# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

class GeneratedPackageTest < Minitest::Test
  def test_free_functions
    assert_respond_to BamlSdk, :initialize!
    assert_respond_to BamlSdk, :hello_world
    assert_respond_to BamlSdk, :single_required_arg
    assert_equal 0, BamlSdk.method(:hello_world).arity
    assert_equal 1, BamlSdk.method(:single_required_arg).arity
    assert_raises(NotImplementedError) { BamlSdk.hello_world }
  end
end
