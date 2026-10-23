# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MapsTest < Minitest::Test
  def test_maps_round_trip_simple_map
    assert_equal({ "a" => 1, "b" => 2 }, BamlSdk::Maps.round_trip_simple_map({ "a" => 1, "b" => 2 }))
  end

  def test_maps_round_trip_list_valued_map
    assert_equal({ "k" => [1, 2] }, BamlSdk::Maps.round_trip_list_valued_map({ "k" => [1, 2] }))
  end

  def test_maps_round_trip_sentiment
    assert_equal BamlSdk::Maps::Sentiment::Positive, BamlSdk::Maps.round_trip_sentiment(BamlSdk::Maps::Sentiment::Positive)
  end

  def test_maps_round_trip_resume
    r = BamlSdk::Maps::Resume.new(name: "n")
    assert_equal r, BamlSdk::Maps.round_trip_resume(r)
  end

end
