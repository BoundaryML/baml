# frozen_string_literal: true

require "minitest/autorun"
require "sorbet-runtime"
$LOAD_PATH.unshift File.expand_path("../../../../sdks/ruby/bridge_ruby/lib", __dir__)
require "baml/bridge"

class ProtocolTest < Minitest::Test
  Wire = BamlBridge::Cffi::V1
  Protocol = Baml::Bridge.const_get(:Protocol, false)

  class Mood < T::Enum
    enums do
      Happy = new("HAPPY")
    end
  end

  class Person < T::Struct
    const :full_name, String
    const :baml_class, T.nilable(String)
  end

  class Group < T::Struct
    const :people, T::Array[T.nilable(Person)]
    const :by_name, T::Hash[String, Person]
    const :mood, Mood
  end

  def setup
    Baml::Bridge.register_type("user.test.Person", Person, fields: { "FullName" => :full_name, "class" => :baml_class })
    Baml::Bridge.register_type("user.test.Group", Group, fields: { "people" => :people, "by_name" => :by_name, "mood" => :mood })
    Baml::Bridge.register_type("user.test.Mood", Mood)
  end

  def test_nested_class_list_map_optional_and_enum_decode
    person = { class_value: { name: "user.test.Person", fields: [
      { key: "FullName", value: { string_value: "Ryan" } },
      { key: "class", value: { union_variant_value: { is_optional: true, value: {} } } }
    ] } }
    result = decode(class_value: { name: "user.test.Group", fields: [
      { key: "people", value: { list_value: { items: [
        { union_variant_value: { is_optional: true, value: person } },
        { union_variant_value: { is_optional: true, value: { null_value: {} } } }
      ] } } },
      { key: "by_name", value: { map_value: {
        key_type: primitive("STRING"), entries: [{ key: "Ryan", value: person }]
      } } },
      { key: "mood", value: { enum_value: { name: "user.test.Mood", value: "HAPPY" } } }
    ] })
    assert_instance_of Group, result
    assert_instance_of Person, result.people.first
    assert_equal "Ryan", result.people.first.full_name
    assert_nil result.people.first.baml_class
    assert_nil result.people.last
    assert_equal "Ryan", result.by_name.fetch("Ryan").full_name
    assert_same Mood::Happy, result.mood
  end

  def test_nested_class_list_map_and_enum_encode
    person = Person.new(full_name: "Ryan", baml_class: nil)
    value = Group.new(people: [person, nil], by_name: { "Ryan" => person }, mood: Mood::Happy)
    # Exercise protobuf serialization as well as the Ruby message constructors.
    encoded = Wire::InboundValue.decode(Wire::InboundValue.encode(Protocol.encode_value(value)))
    assert_equal "user.test.Group", encoded.value_type.class_ty.name
    fields = encoded.class_value.fields.to_h { |field| [field.string_key, field.value] }
    people = fields.fetch("people").list_value.values
    assert_equal "user.test.Person", people.first.value_type.class_ty.name
    assert_equal %w[FullName class], people.first.class_value.fields.map(&:string_key)
    assert_equal "Ryan", people.first.class_value.fields.first.value.string_value
    assert_nil people.first.class_value.fields.last.value.value
    assert_nil people.last.value
    assert_equal "Ryan", fields.fetch("by_name").map_value.entries.first.string_key
    assert_equal "user.test.Person", fields.fetch("by_name").map_value.entries.first.value.value_type.class_ty.name
    assert_equal "user.test.Mood", fields.fetch("mood").enum_value.name
    assert_equal "HAPPY", fields.fetch("mood").enum_value.value
  end

  def test_map_key_types_in_both_directions
    [["INT", "-42", -42, :int_key], ["BOOL", "true", true, :bool_key],
     ["BOOL", "false", false, :bool_key], ["STRING", "key", "key", :string_key]].each do |kind, wire, ruby, field|
      decoded = decode(map_value: { key_type: primitive(kind), entries: [{ key: wire, value: { int_value: 7 } }] })
      assert_equal({ ruby => 7 }, decoded)
      entry = Protocol.encode_value(decoded).map_value.entries.first
      assert_equal ruby, entry.public_send(field)
      assert_equal 7, entry.value.int_value
    end
    decoded = decode(map_value: { key_type: { enum: { name: "user.test.Mood" } }, entries: [
      { key: "HAPPY", value: { bool_value: true } }
    ] })
    assert_equal({ Mood::Happy => true }, decoded)
    enum = Protocol.encode_value(decoded).map_value.entries.first.enum_key
    assert_equal "user.test.Mood", enum.name
    assert_equal "HAPPY", enum.value
  end

  def test_literal_map_keys_decode_as_their_primitive
    [[{ string_value: "k" }, "k", "k"], [{ int_value: 3 }, "3", 3], [{ bool_value: true }, "true", true]].each do |literal, wire, ruby|
      decoded = decode(map_value: { key_type: { literal: literal }, entries: [{ key: wire, value: { int_value: 7 } }] })
      assert_equal({ ruby => 7 }, decoded)
    end
  end

  def test_empty_containers
    assert_empty decode(list_value: {})
    assert_empty decode(map_value: { key_type: primitive("STRING") })
    assert_empty Protocol.encode_value([]).list_value.values
    assert_empty Protocol.encode_value({}).map_value.entries
  end

  def test_unregistered_class_and_enum_names_are_explicit_errors
    [{ class_value: { name: "user.Unknown" } }, { enum_value: { name: "user.Unknown", value: "X" } }].each do |value|
      error = assert_raises(Baml::Bridge::UnsupportedTypeError) { decode(**value) }
      assert_includes error.message, "user.Unknown"
      assert_includes error.message, "not registered"
    end
    error = assert_raises(Baml::Bridge::UnsupportedTypeError) { Protocol.encode_value(Object.new) }
    assert_includes error.message, "Object"
  end

  private

  def primitive(kind)
    { primitive: { kind: "BAML_TY_PRIMITIVE_#{kind}".to_sym } }
  end

  def decode(**fields)
    bytes = Wire::BamlOutboundResult.encode(Wire::BamlOutboundResult.new(ok: fields))
    Protocol.decode_result(bytes)
  end
end
