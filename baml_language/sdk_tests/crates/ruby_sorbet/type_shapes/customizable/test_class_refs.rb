# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ClassRefsTest < Minitest::Test
  def test_class_refs_make_outer
    o = BamlSdk::ClassRefs.make_outer(5)
    assert_equal 5, o.inner.value
  end

  def test_class_refs_round_trip_inner
    i = BamlSdk::ClassRefs::Inner.new(value: 3)
    assert_equal i, BamlSdk::ClassRefs.round_trip_inner(i)
  end

  def test_class_refs_round_trip_outer
    o = BamlSdk::ClassRefs::Outer.new(inner: BamlSdk::ClassRefs::Inner.new(value: 9))
    assert_equal o, BamlSdk::ClassRefs.round_trip_outer(o)
  end

end
