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

  def test_main_namespaces_reachable_via_explicit_import
    require "baml_sdk/lorem"
    require "baml_sdk/ipsum"
    assert_kind_of Module, BamlSdk::Lorem
    assert_kind_of Module, BamlSdk::Ipsum
  end

  def test_main_extract_resume_factory_bindings
    assert_respond_to BamlSdk::Lorem, :extract_resume
    assert_respond_to BamlSdk::Lorem, :extract_resume_async
  end

  def test_main_extract_resume_operation_bindings
    assert_operation_bindings(:extract_resume)
    spec = BamlSdk::Lorem.extract_resume_spec("Ada Lovelace")
    assert_instance_of Baml::FunctionSpec, spec
    assert_respond_to spec, :build_request
    assert_respond_to spec, :build_request_async
    spec.close
    call = BamlSdk::Lorem.extract_resume_spec_async("Ada Lovelace")
    assert_instance_of Baml::Call, call
    assert_instance_of Baml::FunctionSpec, call.wait
    call.wait.close
  end

  def test_main_streaming_extract_factory_bindings
    assert_respond_to BamlSdk::Lorem, :streaming_extract
    assert_respond_to BamlSdk::Lorem, :streaming_extract_async
  end

  def test_main_streaming_extract_operation_bindings
    assert_operation_bindings(:streaming_extract)
  end

  def test_main_classify_sentiment_factory_bindings
    assert_respond_to BamlSdk::Ipsum, :classify_sentiment
    assert_respond_to BamlSdk::Ipsum, :classify_sentiment_async
  end

  def test_main_replay_server_namespace_bindings
    %i[replay_serve_until_shutdown replay_serve_detached].each do |name|
      assert_respond_to BamlSdk::Replay, name
      assert_respond_to BamlSdk::Replay, "#{name}_async"
    end
  end

  private

  def assert_operation_bindings(name)
    %w[spec spec_async stream stream_async].each do |suffix|
      assert_respond_to BamlSdk::Lorem, "#{name}_#{suffix}"
    end
    %w[render_prompt build_request parse].each do |removed|
      refute_respond_to BamlSdk::Lorem, "#{name}__#{removed}"
      refute_respond_to BamlSdk::Lorem, "#{name}__#{removed}_async"
    end
  end
end
