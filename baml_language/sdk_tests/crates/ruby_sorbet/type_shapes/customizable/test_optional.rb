# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class OptionalTest < Minitest::Test
  def test_optional_round_trip_optional_int
    assert_equal 5, BamlSdk::Optional.round_trip_optional_int(5)
    assert_nil BamlSdk::Optional.round_trip_optional_int(nil)
  end

  def test_optional_round_trip_optional_resume
    r = BamlSdk::Optional::Resume.new(name: "ada")
    assert_equal r, BamlSdk::Optional.round_trip_optional_resume(r)
    assert_nil BamlSdk::Optional.round_trip_optional_resume(nil)
  end

  def test_optional_round_trip_optional_union
    assert_equal 3, BamlSdk::Optional.round_trip_optional_union(3)
    assert_equal "s", BamlSdk::Optional.round_trip_optional_union("s")
    assert_nil BamlSdk::Optional.round_trip_optional_union(nil)
  end

  def test_optional_round_trip_resume
    r = BamlSdk::Optional::Resume.new(name: "grace")
    assert_equal r, BamlSdk::Optional.round_trip_resume(r)
  end

  def test_optional_round_trip_optional_container
    c = BamlSdk::Optional::OptionalContainer.new(optional_int: nil, optional_class: BamlSdk::Optional::Resume.new(name: "x"), optional_union: "y")
    assert_equal c, BamlSdk::Optional.round_trip_optional_container(c)
  end

end
