# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MethodsOnClassesTest < Minitest::Test
  Greeter = BamlSdk::MethodsOnClasses::Greeter

  def test_methods_on_classes_method_bindings_exist
    assert_respond_to Greeter, :create
    assert_respond_to Greeter, :create_async
    [:who, :who_async, :greet, :greet_async].each do |name|
      assert_includes Greeter.instance_methods, name
    end
  end

  def test_methods_on_classes_static_create_round_trips
    g = Greeter.create("ada")
    assert_instance_of Greeter, g
    assert_equal "ada", g.name
  end

  def test_methods_on_classes_static_create_async_round_trips
    g = Greeter.create_async("grace").wait
    assert_instance_of Greeter, g
    assert_equal "grace", g.name
  end

  def test_methods_on_classes_instance_who_round_trips
    assert_equal "hopper", Greeter.create("hopper").who
  end

  def test_methods_on_classes_instance_who_async_round_trips
    assert_equal "hopper", Greeter.create_async("hopper").wait.who_async.wait
  end

  def test_methods_on_classes_instance_greet_with_arg_round_trips
    assert_equal "hi", Greeter.create("lovelace").greet("hi")
  end

  def test_methods_on_classes_instance_greet_async_with_arg_round_trips
    assert_equal "hi", Greeter.create_async("lovelace").wait.greet_async("hi").wait
  end
end
