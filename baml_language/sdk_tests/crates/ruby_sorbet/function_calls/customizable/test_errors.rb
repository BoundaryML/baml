# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ErrorsTest < Minitest::Test
  def test_errors_user_panic_surfaces_as_baml_panic
    error = assert_raises(Baml::PanicError) { BamlSdk::ThrowsTest.do_panic("user-initiated boom") }
    # Ruby exposes the type name and message, not Python's decoded UserPanic instance.
    assert_equal "baml.panics.UserPanic", error.type_name
    assert_equal "user-initiated boom", error.message
    refute_kind_of Baml::Error, error
  end

  def test_errors_union_throws_preserves_class_name
    single = assert_raises(Baml::Error) { BamlSdk::RaisesTest.reparse("x") }
    union = assert_raises(Baml::Error) { BamlSdk::RaisesTest.load_doc("x") }
    # Ruby does not decode the ParseError payload or generate DocLoader.load yet.
    assert_equal "user.raises_test.ParseError", single.type_name
    assert_equal single.type_name, union.type_name
    assert_equal "x", single.message
    assert_equal single.message, union.message
  end

  # SDK_PARITY_LINT(skip): Ruby error wrapper for a thrown string, without a decoded payload
  def test_primitive_thrown_error
    error = assert_raises(Baml::Error) { BamlSdk.unhandled_spawn_error }
    assert_equal "string", error.type_name
    assert_equal "boom", error.message
  end
end
