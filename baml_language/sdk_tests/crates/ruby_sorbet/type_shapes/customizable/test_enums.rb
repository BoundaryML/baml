# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class EnumsTest < Minitest::Test
  def test_enums_pick_sentiment
    assert_equal BamlSdk::Enums::Sentiment::Positive, BamlSdk::Enums.pick_sentiment(true)
    assert_equal BamlSdk::Enums::Sentiment::Negative, BamlSdk::Enums.pick_sentiment(false)
  end

  def test_enums_pick_positive
    assert_equal BamlSdk::Enums::Sentiment::Positive, BamlSdk::Enums.pick_positive()
  end

  def test_enums_round_trip_sentiment
    assert_equal BamlSdk::Enums::Sentiment::Negative, BamlSdk::Enums.round_trip_sentiment(BamlSdk::Enums::Sentiment::Negative)
  end

  def test_enums_round_trip_sentiment_positive
    assert_equal BamlSdk::Enums::Sentiment::Positive, BamlSdk::Enums.round_trip_sentiment_positive(BamlSdk::Enums::Sentiment::Positive)
  end

  def test_enums_round_trip_enums
    e = BamlSdk::Enums::Enums.new(bare_enum: BamlSdk::Enums::Sentiment::Positive, variant_as_type: BamlSdk::Enums::Sentiment::Positive)
    assert_equal e, BamlSdk::Enums.round_trip_enums(e)
  end

end
