# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MethodsOnClassesTest < Minitest::Test
  Greeter = BamlSdk::MethodsOnClasses::Greeter

  # SDK_PARITY_LINT(skip): Ruby lifecycle hook names must not run while requiring the generated SDK
  def test_ruby_lifecycle_hooks_generate_and_load
    assert_equal 1, BamlSdk::MethodsOnClasses.baml_singleton_method_added
    assert_equal 2, BamlSdk::MethodsOnClasses::HookNames.baml_method_added
  end

  # SDK_PARITY_LINT(skip): Ruby Object properties and Module callables need escaped names
  def test_ruby_object_and_module_names_generate_load_and_call
    type = BamlSdk::MethodsOnClasses::HookNames
    value = type.new(baml_display: "display", baml_tap: "tap", then_: "then", baml_gem: "gem")
    result = BamlSdk::MethodsOnClasses.baml_name(value)
    assert_equal "display", result.baml_display
    assert_equal "tap", result.baml_tap
    assert_equal "then", result.then_
    assert_equal "gem", result.baml_gem
    assert_equal 3, type.baml_ancestors
    assert_equal 4, type.baml_decorator
  end

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
