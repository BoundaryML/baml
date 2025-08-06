require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"

# TODO: Right now this does not work because removal / listing methods are not exposed.
describe "TypeBuilder APIs" do
  it "should reset" do
    tb = Baml::TypeBuilder.new
    tb.Person.add_property("last_name", tb.string.list)
    tb.Person.add_property("height", tb.float.optional).description("Height in meters")
    tb.reset

    person_props_after_tb_clear = tb.Person.list_properties

    refute person_props_after_tb_clear.key?("last_name")
    refute person_props_after_tb_clear.key?("height")
  end

  it "should reset a class" do
    tb = Baml::TypeBuilder.new
    tb.Person.add_property("last_name", tb.string.list)
    tb.Person.add_property("height", tb.float.optional).description("Height in meters")

    tb.DynamicOutput.add_property("hair_color", tb.string)
    tb.DynamicOutput.add_property("height", tb.float.optional).description("Height in meters")

    tb.Person.reset

    person_props_after_class_clear = tb.Person.list_properties
    dynamic_output_props_after_class_clear = tb.DynamicOutput.list_properties

    refute person_props_after_class_clear.key?("last_name")
    refute person_props_after_class_clear.key?("height")

    assert dynamic_output_props_after_class_clear.key?("hair_color")
    assert dynamic_output_props_after_class_clear.key?("height")
  end

  it "should remove a property from a class" do
    tb = Baml::TypeBuilder.new
    tb.Person.add_property("last_name", tb.string.list)
    tb.Person.add_property("height", tb.float.optional).description("Height in meters")

    tb.Person.remove_property("last_name")

    person_props = tb.Person.list_properties

    refute person_props.key?("last_name")
    assert person_props.key?("height")
  end

  it "should reset a dynamically added class" do
    tb = Baml::TypeBuilder.new
    person_class = tb.add_class("AddedPerson")
    person_class.add_property("last_name", tb.string.list)
    person_class.add_property("height", tb.float.optional).description("Height in meters")

    person_class.reset

    person_props = person_class.list_properties

    refute person_props.key?("last_name")
    refute person_props.key?("height")
  end

  it "should remove a property from a dynamically added class" do
    tb = Baml::TypeBuilder.new
    person_class = tb.add_class("AddedPerson")
    person_class.add_property("last_name", tb.string.list)
    person_class.add_property("height", tb.float.optional).description("Height in meters")

    person_class.remove_property("last_name")

    person_props = person_class.list_properties

    refute person_props.key?("last_name")
    assert person_props.key?("height")
  end

  it "should get property types from a class" do
    tb = Baml::TypeBuilder.new
    tb.Person.add_property("last_name", tb.string.list)
    tb.Person.add_property("height", tb.float.optional).description("Height in meters")

    props = tb.Person.list_properties

    assert_equal tb.string.list, props["last_name"]
    assert_equal tb.float.optional, props["height"]
  end
end
