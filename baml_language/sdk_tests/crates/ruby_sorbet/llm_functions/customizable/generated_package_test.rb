# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

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
    error = assert_raises(NotImplementedError) { method.call("text") }
    assert_includes error.message, "lorem.stream_e2e_collect_doc"
    assert BamlSdk::BYTECODE.frozen?
    assert_equal Encoding::BINARY, BamlSdk::BYTECODE.encoding
  end
end
