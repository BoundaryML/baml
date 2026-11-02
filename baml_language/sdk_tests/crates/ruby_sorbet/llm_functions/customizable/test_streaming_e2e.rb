# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
require_relative "replay_harness"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class StreamingE2ETest < Minitest::Test
  def test_streaming_e2e_stream
    ReplayHarness.with_server("replay_extract_string") do
      stream = BamlSdk::Lorem.stream_e2e_extract_stream("ignored-by-replay-server")
      assert_instance_of Baml::Stream, stream
      assert_partials(stream, String)
      assert_instance_of String, stream.final
      stream.close

      stream = BamlSdk::Lorem.stream_e2e_extract_stream("ignored-by-replay-server")
      assert_instance_of Enumerator, stream.each
      results = 0
      returned = stream.each do |partial|
        assert_instance_of String, partial
        results += 1
        assert_operator results, :<, 10_000, "stream.each failed to terminate"
      end
      assert_same stream, returned
      assert_operator results, :>=, 10
      assert_instance_of String, stream.final
      stream.close

      spec = BamlSdk::Lorem.stream_e2e_extract_spec("ignored-by-replay-server")
      request = spec.build_request
      assert_instance_of BamlSdk::Baml::Http::Request, request
      assert_equal "POST", request.baml_method
      assert_equal "#{ENV.fetch('BAML_REPLAY_BASE_URL')}/responses", request.url
      spec.close
    end
  end

  def test_streaming_e2e_stream_async
    ReplayHarness.with_server("replay_extract_string") do
      call = BamlSdk::Lorem.stream_e2e_extract_stream_async("ignored-by-replay-server")
      assert_instance_of Baml::Call, call
      stream = call.wait
      assert_instance_of Baml::Stream, stream
      assert_partials(stream, String, asynchronous: true)
      final = stream.final_async
      assert_instance_of Baml::Call, final
      assert_instance_of String, final.wait
      stream.close

      spec = BamlSdk::Lorem.stream_e2e_extract_spec_async("ignored-by-replay-server").wait
      request = spec.build_request_async
      assert_instance_of Baml::Call, request
      assert_instance_of BamlSdk::Baml::Http::Request, request.wait
      assert_equal "#{ENV.fetch('BAML_REPLAY_BASE_URL')}/responses", request.wait.url
      spec.close
    end
  end

  def test_streaming_e2e_stream_doc
    ReplayHarness.with_server("replay_extract_doc") do
      stream = BamlSdk::Lorem.stream_e2e_extract_doc_stream("ignored-by-replay-server")
      assert_instance_of Baml::Stream, stream
      assert_partials(stream, BamlSdk::Lorem::StreamingDoc)
      assert_instance_of BamlSdk::Lorem::StreamingDoc, stream.final
      stream.close
    end
  end

  def test_streaming_e2e_stream_doc_async
    ReplayHarness.with_server("replay_extract_doc") do
      call = BamlSdk::Lorem.stream_e2e_extract_doc_stream_async("ignored-by-replay-server")
      assert_instance_of Baml::Call, call
      stream = call.wait
      assert_instance_of Baml::Stream, stream
      assert_partials(stream, BamlSdk::Lorem::StreamingDoc, asynchronous: true)
      final = stream.final_async
      assert_instance_of Baml::Call, final
      assert_instance_of BamlSdk::Lorem::StreamingDoc, final.wait
      stream.close
    end
  end

  def test_streaming_e2e_stream_collect_in_baml
    ReplayHarness.with_server("replay_extract_string") do
      result = BamlSdk::Lorem.stream_e2e_collect("ignored-by-replay-server")
      assert_instance_of BamlSdk::Lorem::StreamE2ECollectResult, result
      assert_operator result.next_calls.length, :>=, 10
      result.next_calls.each { |value| assert_instance_of String, value }
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

  private

  def assert_partials(stream, type, asynchronous: false)
    Timeout.timeout(30) do
      results = 0
      loop do
        value = if asynchronous
                  call = stream.next_async
                  assert_instance_of Baml::Call, call
                  call.wait
                else
                  stream.next
                end
        break if value.is_a?(BamlSdk::Ai::Stream::Done)

        results += 1
        assert_instance_of type, value
        assert_operator results, :<, 10_000, "stream.next failed to terminate"
      end
      assert_operator results, :>=, 10, "expected at least 10 partials"
    end
  end
end
