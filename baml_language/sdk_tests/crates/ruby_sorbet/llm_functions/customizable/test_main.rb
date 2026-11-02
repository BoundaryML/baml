# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

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

  def test_main_replay_server_namespace_bindings
    %i[replay_serve_until_shutdown replay_serve_detached].each do |name|
      assert_respond_to BamlSdk::Replay, name
      assert_respond_to BamlSdk::Replay, "#{name}_async"
    end
  end
end
