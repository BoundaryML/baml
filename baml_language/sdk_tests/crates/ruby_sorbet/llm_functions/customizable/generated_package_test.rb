# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
require_relative "replay_harness"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class GeneratedPackageTest < Minitest::Test
  def test_declarations
    assert_respond_to BamlSdk, :initialize!
    assert_operator BamlSdk::Lorem::StreamingDoc, :<, T::Struct
    props = BamlSdk::Lorem::StreamingDoc.props
    assert_equal String, props.fetch(:title).fetch(:type)
    assert_equal Integer, props.fetch(:word_count).fetch(:type)
    assert_equal "T.nilable(String)", props.fetch(:body).fetch(:type_object).to_s
    doc = BamlSdk::Lorem::StreamingDoc.new(title: "title", body: nil, word_count: 1)
    refute_respond_to doc, :title=
    assert_raises(TypeError) { BamlSdk::Lorem::StreamingDoc.new(title: 1, word_count: 1) }
    assert_operator BamlSdk::Ipsum::Sentiment, :<, T::Enum
    assert_equal %w[NEGATIVE NEUTRAL POSITIVE], BamlSdk::Ipsum::Sentiment.values.map(&:serialize).sort
    method = BamlSdk::Lorem.method(:stream_e2e_collect_doc)
    assert_equal 1, method.arity
    assert_equal [:req], method.parameters.map(&:first).reject { |kind| kind == :block }
    assert_raises(TypeError) { method.call(1) }
    assert BamlSdk::BYTECODE.frozen?
    assert_equal Encoding::BINARY, BamlSdk::BYTECODE.encoding
  end

  def test_stream_e2e_collect_doc_returns_the_declared_class
    ReplayHarness.with_server("replay_extract_doc") do
      doc = BamlSdk::Lorem.stream_e2e_collect_doc("ignored-by-replay-server")
      assert_instance_of BamlSdk::Lorem::StreamingDoc, doc
      assert_instance_of String, doc.title
      assert_instance_of Integer, doc.word_count
      assert doc.body.nil? || doc.body.is_a?(String)
    end
    assert_nil ENV["BAML_REPLAY_BASE_URL"]
    assert_nil ENV["BAML_REPLAY_API_KEY"]
  end

  def test_stream_e2e_collect_returns_typed_partials_and_final
    ReplayHarness.with_server("replay_extract_doc") do
      result = BamlSdk::Lorem.stream_e2e_collect("ignored-by-replay-server")
      assert_instance_of BamlSdk::Lorem::StreamE2ECollectResult, result
      assert_instance_of String, result.final_call
      assert_operator result.next_calls.length, :>=, 10
      assert result.next_calls.all? { |value| value.nil? || value.is_a?(String) }
    end
  end

  def test_replay_server_failure_surfaces_and_cleans_up
    addr_file = nil
    failure = RuntimeError.new("server startup failed")
    serve = lambda do |_recording, path|
      addr_file = path
      raise failure
    end
    BamlSdk::Replay.stub(:replay_serve_until_shutdown, serve) do
      error = Timeout.timeout(2) do
        assert_raises(RuntimeError) do
          ReplayHarness.with_server("replay_extract_doc") { flunk "server should not start" }
        end
      end
      assert_same failure, error
    end
    refute File.exist?(addr_file)
    assert_nil ENV["BAML_REPLAY_BASE_URL"]
    assert_nil ENV["BAML_REPLAY_API_KEY"]
  end

  def test_sentiment_wire_values_use_the_generated_enum
    wire = BamlBridge::Cffi::V1
    protocol = Baml::Bridge.const_get(:Protocol, false)
    BamlSdk::Ipsum::Sentiment.values.each do |sentiment|
      value = wire::BamlOutboundValue.new(enum_value: {
        name: "user.ipsum.Sentiment", value: sentiment.serialize
      })
      assert_same sentiment, protocol.decode_value(value)
      encoded = protocol.encode_value(sentiment).enum_value
      assert_equal "user.ipsum.Sentiment", encoded.name
      assert_equal sentiment.serialize, encoded.value
    end
  end
end
