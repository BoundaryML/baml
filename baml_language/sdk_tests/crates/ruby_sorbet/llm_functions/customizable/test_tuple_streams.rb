# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

# Each function's client replays fixed text deltas, so these streams run the
# ordinary partial and final parse offline. Tuples arrive as frozen Arrays.
class TupleStreamsTest < Minitest::Test
  Streams = BamlSdk::TupleStreams
  # The carried tuple type is internal; the test reads it to prove it survives.
  CARRIED = Baml::Bridge.const_get(:Protocol)

  def test_tuples_stream_items
    stream = Streams.stream_tuple_items_stream("x")
    partials = stream.each.to_a
    refute_empty partials
    assert_equal [["a", 1]], partials.first
    partials.each do |partial|
      assert_equal [["a", 1], ["b", 2]].first(partial.length), partial
      partial.each { |item| assert_predicate item, :frozen? }
    end
    final = stream.final
    assert_equal [["a", 1], ["b", 2]], final
    assert_equal [[:tuple, [[:string], [:int]]]] * 2, final.map { |item| CARRIED.tuple_type(item) }
    stream.close
    assert_equal final, Streams.stream_tuple_items("x")
  end

  def test_tuples_stream_final
    stream = Streams.stream_tuple_final_stream("x")
    partials = stream.each.to_a
    refute_empty partials
    partials.each do |partial|
      assert_predicate partial, :frozen?
      assert_equal 1, partial[0]
      assert "a".start_with?(partial[1])
    end
    assert_equal [1, "a"], partials.last
    final = stream.final
    assert_equal [1, "a"], final
    assert_equal [:tuple, [[:int], [:string]]], CARRIED.tuple_type(final)
    stream.close
    assert_equal [1, "a"], Streams.stream_tuple_final_async("x").wait
  end
end
