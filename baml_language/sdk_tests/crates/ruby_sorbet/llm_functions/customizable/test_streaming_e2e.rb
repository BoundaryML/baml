# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
require_relative "replay_harness"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class StreamingE2ETest < Minitest::Test
  def test_streaming_e2e_stream_collect_in_baml
    ReplayHarness.with_server("replay_extract_string") do
      result = BamlSdk::Lorem.stream_e2e_collect("ignored-by-replay-server")
      assert_instance_of BamlSdk::Lorem::StreamE2ECollectResult, result
      assert_operator result.next_calls.length, :>=, 10
      assert result.next_calls.all? { |value| value.nil? || value.is_a?(String) }
      assert_instance_of String, result.final_call
    end
  end

  def test_streaming_e2e_stream_doc_collect_in_baml
    ReplayHarness.with_server("replay_extract_doc") do
      doc = BamlSdk::Lorem.stream_e2e_collect_doc("ignored-by-replay-server")
      assert_instance_of BamlSdk::Lorem::StreamingDoc, doc
      assert_respond_to doc, :title
      assert_instance_of String, doc.title
      assert_instance_of Integer, doc.word_count
      assert doc.body.nil? || doc.body.is_a?(String)
    end
  end
end
