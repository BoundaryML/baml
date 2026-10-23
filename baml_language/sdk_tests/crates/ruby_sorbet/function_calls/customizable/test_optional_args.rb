# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class OptionalArgsTest < Minitest::Test
  def test_optional_args_runtime_matrix
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe(1)
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe(1, opt1: Baml::UNSET, opt2: Baml::UNSET)
    # Required arguments are positional in Ruby, as in Java's port.
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe(1)
    assert_equal [1, nil, 99], BamlSdk.optional_args_probe(1, opt1: nil)
    assert_equal [1, 8, 99], BamlSdk.optional_args_probe(1, opt1: 8)
    assert_equal [1, 5, nil], BamlSdk.optional_args_probe(1, opt2: nil)
    assert_equal [1, 5, 9], BamlSdk.optional_args_probe(1, opt2: 9)
    assert_equal [1, nil, nil], BamlSdk.optional_args_probe(1, opt1: nil, opt2: nil)
    assert_equal [1, 8, 9], BamlSdk.optional_args_probe(1, opt1: 8, opt2: 9)
  end

  def test_optional_args_python_unset_and_none_differ_in_one_call
    refute_nil Baml::UNSET
    assert_equal [1, 5, nil], BamlSdk.optional_args_probe(1, opt1: Baml::UNSET, opt2: nil)
    assert_equal [1, nil, 99], BamlSdk.optional_args_probe(1, opt1: nil, opt2: Baml::UNSET)
  end

  def test_optional_args_async_samples
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe_async(1).wait
    assert_equal [1, nil, 99], BamlSdk.optional_args_probe_async(1, opt1: nil).wait
    assert_equal [1, 5, 9], BamlSdk.optional_args_probe_async(1, opt2: 9).wait
  end

  def test_optional_args_opt_box_method_matrix
    box = BamlSdk::OptBox.make(10)
    assert_equal 17, box.base
    box2 = BamlSdk::OptBox.make(10, opt1: 0)
    assert_equal 10, box2.base
    assert_equal [10, 1, 5], box2.probe(1)
    assert_equal [10, 1, 8], box2.probe(1, opt1: 8)
  end

  def test_optional_args_negative_runtime_cases_reject
    assert_raises(ArgumentError) { BamlSdk.optional_args_probe(1, opt3: 1) }
    assert_raises(ArgumentError) { BamlSdk.optional_args_probe }
    assert_raises(ArgumentError) { BamlSdk.optional_args_probe(1, arg0: 1) }
    # Nullable parameters without a BAML default are still required.
    assert_raises(ArgumentError) { BamlSdk::StaticMethodEdges::Edge.nullable }
    assert_nil BamlSdk::StaticMethodEdges::Edge.nullable(nil)
  end
end
