# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"

ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class JsonTest < Minitest::Test
  def test_json_returned_from_host_callback_supports_typed_narrowing
    assert_equal "object", BamlSdk::GoJsonTests.json_callback_kind(->(value) { { "wrapped" => value } }, "payload")
  end

  def test_host_supplied_json_supports_typed_narrowing
    json = BamlSdk::GoJsonTests
    obj = { "type" => "ok", "nested" => { "list" => [1, { "deep" => "found" }] } }
    assert_equal "object", json.json_kind(obj)
    assert_equal "array", json.json_kind([1])
    assert_equal "string", json.json_kind("text")
    assert_equal "other", json.json_kind(3)
    assert_equal "ok", json.json_path_string(obj, ".type")
    assert_equal "found", json.json_path_string(obj, ".nested.list[1].deep")
    assert_equal "fallback", json.json_path_string_or(obj, ".missing", "fallback")
    error = assert_raises(Baml::Error) { json.json_path_string(obj, ".absent") }
    assert_includes error.message, "missing field"
  end
end
