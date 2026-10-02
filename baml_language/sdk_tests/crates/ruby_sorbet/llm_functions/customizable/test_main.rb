# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_RUNTIME_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class MainTest < Minitest::Test
  def test_main_lorem_resume_class_shape
    assert_operator BamlSdk::Lorem::Resume, :<, T::Struct
    assert_equal %i[email name], BamlSdk::Lorem::Resume.props.keys.sort
  end

  def test_main_lorem_streaming_doc_class_shape
    assert_operator BamlSdk::Lorem::StreamingDoc, :<, T::Struct
    props = BamlSdk::Lorem::StreamingDoc.props
    assert_equal %i[body title word_count], props.keys.sort
    assert_equal String, props.fetch(:title).fetch(:type)
    assert_equal Integer, props.fetch(:word_count).fetch(:type)
    assert_equal "T.nilable(String)", props.fetch(:body).fetch(:type_object).to_s
    doc = BamlSdk::Lorem::StreamingDoc.new(title: "title", body: nil, word_count: 1)
    refute_respond_to doc, :title=
    assert_raises(TypeError) { BamlSdk::Lorem::StreamingDoc.new(title: 1, word_count: 1) }
  end

  def test_main_ipsum_sentiment_enum_shape
    assert_operator BamlSdk::Ipsum::Sentiment, :<, T::Enum
    assert_equal %w[NEGATIVE NEUTRAL POSITIVE], BamlSdk::Ipsum::Sentiment.values.map(&:serialize).sort
    # T::Enum members serialize to strings; unlike Python's enum, they aren't strings themselves.
    assert_equal "POSITIVE", BamlSdk::Ipsum::Sentiment::POSITIVE.serialize
    assert_instance_of String, BamlSdk::Ipsum::Sentiment::POSITIVE.serialize
  end

  # SDK_PARITY_LINT(skip): Ruby generates only sync replay bindings, not Python's async siblings
  def test_replay_server_sync_namespace_bindings
    assert_respond_to BamlSdk::Replay, :replay_serve_until_shutdown
    assert_respond_to BamlSdk::Replay, :replay_serve_detached
  end

  # SDK_PARITY_LINT(skip): Ruby package initialization, bytecode, and Sorbet method signature
  def test_generated_package_contract
    assert_respond_to BamlSdk, :initialize!
    method = BamlSdk::Lorem.method(:stream_e2e_collect_doc)
    assert_equal 1, method.arity
    assert_equal [:req], method.parameters.map(&:first).reject { |kind| kind == :block }
    assert_raises(TypeError) { method.call(1) }
    assert BamlSdk::BYTECODE.frozen?
    assert_equal Encoding::BINARY, BamlSdk::BYTECODE.encoding
  end

  # SDK_PARITY_LINT(skip): Ruby T::Enum registration and protobuf encoding and decoding
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
