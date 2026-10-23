# frozen_string_literal: true
require "minitest/autorun"
require "baml_sdk"
ENV["BAML_BRIDGE_PATH"] = ENV.fetch("BAML_RUBY_TEST_REAL_RUNTIME")

class PrimitivesTest < Minitest::Test
  def test_primitives_return_int
    assert_equal 42, BamlSdk::Primitives.return_int()
  end

  def test_primitives_return_float
    assert_equal 3.14, BamlSdk::Primitives.return_float()
  end

  def test_primitives_return_string
    assert_equal "hello", BamlSdk::Primitives.return_string()
  end

  def test_primitives_return_bool
    assert_equal true, BamlSdk::Primitives.return_bool()
  end

  def test_primitives_return_bigint
    assert_equal 12345678901234567890, BamlSdk::Primitives.return_bigint()
  end

  def test_primitives_return_null
    assert_nil BamlSdk::Primitives.return_null()
  end

  def test_primitives_round_trip_int
    assert_equal 7, BamlSdk::Primitives.round_trip_int(7)
  end

  def test_primitives_round_trip_float
    assert_equal 2.5, BamlSdk::Primitives.round_trip_float(2.5)
  end

  def test_primitives_round_trip_float_accepts_int
    result = BamlSdk::Primitives.round_trip_float(7)
    assert_instance_of Float, result
    assert_equal 7.0, result
  end

  def test_primitives_round_trip_string
    assert_equal "hi", BamlSdk::Primitives.round_trip_string("hi")
  end

  def test_primitives_round_trip_bool
    assert_equal false, BamlSdk::Primitives.round_trip_bool(false)
  end

  def test_primitives_round_trip_bigint
    big = 2**80
    assert_equal big, BamlSdk::Primitives.round_trip_bigint(big)
    assert_equal -big, BamlSdk::Primitives.round_trip_bigint(-big)
    hex_boundary = 2**64
    assert_equal hex_boundary, BamlSdk::Primitives.round_trip_bigint(hex_boundary)
  end

  def test_primitives_round_trip_null
    assert_nil BamlSdk::Primitives.round_trip_null(nil)
  end

  def test_primitives_round_trip_uint8_array
    assert_equal "\x00\x01\x02".b, BamlSdk::Primitives.round_trip_uint8_array("\x00\x01\x02".b)
    assert_equal Encoding::BINARY, BamlSdk::Primitives.round_trip_uint8_array("ab").encoding

    # Ruby has one String type: only binary strings choose bytes when both
    # string and bytes occur in a union, including nested containers/aliases.
    protocol = Baml::Bridge.const_get(:Protocol, false)
    union = [:union, [[:string], [:bytes]]]
    assert_equal :string_value, protocol.encode_value("ab", union).value
    assert_equal :uint8array_value, protocol.encode_value("ab".b, union).value
    protocol.define_alias("user.ByteChoice", union)
    hint = [:map, [:string], [:list, [:alias, "user.ByteChoice"]]]
    values = protocol.encode_value({ "k" => ["text", "bytes".b] }, hint)
      .map_value.entries.first.value.list_value.values
    assert_equal [:string_value, :uint8array_value], values.map(&:value)
  end

  def test_primitives_round_trip_primitives
    p = BamlSdk::Primitives::Primitives.new(int_field: 1, float_field: 1.5, string_field: "s", bool_field: true, null_field: nil, uint8array_field: "\x61\x62".b)
    assert_equal p, BamlSdk::Primitives.round_trip_primitives(p)
  end

  def test_primitives_round_trip_primitives_float_field_accepts_int
    p = BamlSdk::Primitives::Primitives.new(int_field: 1, float_field: 2, string_field: "s", bool_field: true, null_field: nil, uint8array_field: "\x61\x62".b)
    assert_equal 2, p.float_field # Ruby accepts Integer input; the engine widens it.
    result = BamlSdk::Primitives.round_trip_primitives(p)
    assert_equal 2.0, result.float_field
    assert_instance_of Float, result.float_field
  end

end
