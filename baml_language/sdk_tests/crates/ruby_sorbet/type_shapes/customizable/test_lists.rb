# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class ListsTest < Minitest::Test
  def test_lists_round_trip_ints
    assert_equal [1, 2, 3], BamlSdk::Lists.round_trip_ints([1, 2, 3])
  end

  def test_lists_round_trip_empty_list
    assert_equal [], BamlSdk::Lists.round_trip_ints([])
  end

  def test_lists_round_trip_optional_strings
    assert_equal ["a", nil, "b"], BamlSdk::Lists.round_trip_optional_strings(["a", nil, "b"])
  end

  def test_lists_round_trip_union_list
    assert_equal [1, "two", 3], BamlSdk::Lists.round_trip_union_list([1, "two", 3])
  end

  def test_lists_round_trip_list_container
    c = BamlSdk::Lists::ListContainer.new(ints: [1, 2], optional_strings: [nil, "z"], union_list: [1, "x"])
    assert_equal c, BamlSdk::Lists.round_trip_list_container(c)
  end

end
