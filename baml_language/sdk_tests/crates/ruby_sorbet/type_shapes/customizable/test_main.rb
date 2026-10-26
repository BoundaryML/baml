# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MainTest < Minitest::Test
  def test_main_root_imports_cleanly
    assert_kind_of Module, BamlSdk
  end

  def test_main_all_namespaces_reachable
    %i[Primitives Media Enums Literals ClassRefs Aliases AliasesConsumer Optional
       Lists Maps Unions Recursion Generics ForwardRefs ComplexModels Lorem A].each do |name|
      assert_kind_of Module, BamlSdk.const_get(name, false)
    end
  end

  def test_main_root_foo_reachable
    assert_operator BamlSdk::Foo, :<, T::Struct
  end

  def test_main_lorem_resume_reachable
    assert_operator BamlSdk::Lorem::Resume, :<, T::Struct
  end

  def test_main_deep_namespace_thing_reachable
    assert_operator BamlSdk::A::B::Thing, :<, T::Struct
  end
end
