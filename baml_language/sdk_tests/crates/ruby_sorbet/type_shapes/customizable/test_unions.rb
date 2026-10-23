# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class UnionsTest < Minitest::Test
  def test_unions_round_trip_null_to_end
    assert_equal 1, BamlSdk::Unions.round_trip_null_to_end(1)
    assert_equal "s", BamlSdk::Unions.round_trip_null_to_end("s")
    assert_nil BamlSdk::Unions.round_trip_null_to_end(nil)
  end

  def test_unions_round_trip_dedup
    assert_equal 2, BamlSdk::Unions.round_trip_dedup(2)
    assert_equal "x", BamlSdk::Unions.round_trip_dedup("x")
  end

  def test_unions_round_trip_singleton_unwrap
    assert_equal 7, BamlSdk::Unions.round_trip_singleton_unwrap(7)
  end

  def test_unions_round_trip_optional_plus_null
    assert_equal BamlSdk::Unions::T.new(v: 1), BamlSdk::Unions.round_trip_optional_plus_null(BamlSdk::Unions::T.new(v: 1))
    assert_equal "s", BamlSdk::Unions.round_trip_optional_plus_null("s")
    assert_nil BamlSdk::Unions.round_trip_optional_plus_null(nil)
  end

  def test_unions_round_trip_str_or_int_list
    assert_equal [1, 2, 3], BamlSdk::Unions.round_trip_str_or_int_list([1, 2, 3])
    assert_equal ["a", "b"], BamlSdk::Unions.round_trip_str_or_int_list(["a", "b"])
    assert_equal [], BamlSdk::Unions.round_trip_str_or_int_list([])
  end

  def test_unions_round_trip_t
    assert_equal BamlSdk::Unions::T.new(v: 4), BamlSdk::Unions.round_trip_t(BamlSdk::Unions::T.new(v: 4))
  end

  def test_unions_round_trip_union_container
    c = BamlSdk::Unions::UnionContainer.new(null_to_end: nil, dedup: "d", singleton_unwrap: 5, optional_plus_null: BamlSdk::Unions::T.new(v: 2))
    assert_equal c, BamlSdk::Unions.round_trip_union_container(c)
  end

end
