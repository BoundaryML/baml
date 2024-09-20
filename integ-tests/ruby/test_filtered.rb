require 'json'
require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"


b = Baml.Client

describe "ruby<->baml integration tests (filtered)" do
  it "tests dynamic enum output" do
    t = Baml::TypeBuilder.new
    t.DynEnumTwo.add_value("positive").description("The feedback expresses satisfaction or praise.")
    t.DynEnumTwo.add_value("neutral").description("The feedback is neither clearly positive nor negative.")
    t.DynEnumTwo.add_value("negative").description("The feedback expresses dissatisfaction or complaints.")

    # TODO: figure out a non-naive impl of #list_properties in Ruby
    # puts t.DynamicOutput.list_properties
    # t.DynamicOutput.list_properties.each do |prop|
    #   puts "Property: #{prop}"
    # end

    output = b.ClassifyDynEnumTwo(
      input: "My name is Harrison. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: t} 
    )
    puts output.inspect
  end

end