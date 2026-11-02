# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class InvocationTest < Minitest::Test
  def test_four_call_forms
    options = Baml::Options.new(timeout_ms: 1000)
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe(1)
    assert_equal [1, 7, 99], BamlSdk.optional_args_probe(1, opt1: 7)
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe(1, _baml: options)
    assert_equal [1, 7, 99], BamlSdk.optional_args_probe(1, opt1: 7, _baml: options)
  end

  def test_four_call_forms_async
    options = Baml::Options.new(timeout_ms: 1000)
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe_async(1).wait
    assert_equal [1, 7, 99], BamlSdk.optional_args_probe_async(1, opt1: 7).wait
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe_async(1, _baml: options).wait
    assert_equal [1, 7, 99], BamlSdk.optional_args_probe_async(1, opt1: 7, _baml: options).wait
  end

  def test_empty_controls
    assert_equal "hello world", BamlSdk.hello_world
    assert_equal "hello world", BamlSdk.hello_world(_baml: Baml::Options.new)
    assert_equal "hello world", BamlSdk.hello_world(_baml: nil)
    assert_equal "hello world", BamlSdk.hello_world(_baml: Baml::Options.new(trace: nil, cancel: nil, timeout_ms: nil))
  end

  def test_omitted_argument_is_not_null
    assert_equal [1, 5, 99], BamlSdk.optional_args_probe(1, _baml: Baml::Options.new)
    assert_equal [1, nil, 99], BamlSdk.optional_args_probe(1, opt1: nil, _baml: Baml::Options.new)
  end

  def test_invalid_timeout_rejected
    [-1, 0.5, Float::INFINITY, Float::NAN, 2_147_483_648, true].each do |timeout|
      assert_raises(ArgumentError) { BamlSdk.hello_world(_baml: Baml::Options.new(timeout_ms: timeout)) }
    end
    assert_raises(TypeError) { BamlSdk.hello_world(_baml: {}) }
    assert_raises(ArgumentError) { Baml::Options.new(trace: Object.new) }
    assert_raises(TypeError) { BamlSdk.hello_world(_baml: Baml::Options.new(cancel: Object.new)) }
  end

  def test_timeout_upper_bound_accepted
    assert_equal "hello world", BamlSdk.hello_world(_baml: Baml::Options.new(timeout_ms: 2_147_483_647))
  end

  def test_methods_accept_controls
    options = Baml::Options.new
    box = BamlSdk::OptBox.make(1, _baml: options)
    assert_equal 8, box.base
    assert_equal [8, 2, 5], box.probe(2, _baml: options)
    assert_equal [8, 2, nil], box.probe(2, opt1: nil, _baml: options)
  end

  def test_composite_token_observes_every_source
    [0, 1].each do |index|
      sources = Array.new(2) { BamlSdk::Baml::Spawn::CancelToken.baml_new }
      combined = BamlSdk::Baml::Spawn::CancelToken.any(sources)
      sources[index].cancel
      assert combined.is_cancelled
      error = assert_raises(Baml::CancelledError) do
        BamlSdk.hello_world_async(_baml: Baml::Options.new(cancel: combined)).wait
      end
      assert_instance_of BamlSdk::Baml::Panics::Cancelled, error.value
      refute sources[1 - index].is_cancelled
    end
  end
end
