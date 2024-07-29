require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"


b = Baml.Client

describe "ruby<->baml integration tests" do
  it "works with all inputs" do
    res = b.TestFnNamedArgsSingleBool(myBool: true)
    assert_equal res, "true"

    res = b.TestFnNamedArgsSingleStringList(myArg: ["a", "b", "c"])
    assert_includes res, "a"
    assert_includes res, "b"
    assert_includes res, "c"

    res = b.TestFnNamedArgsSingleClass(
        myArg: Baml::Types::NamedArgsSingleClass.new(
            key: "key",
            key_two: true,
            key_three: 52,
        )
    )
    assert_includes res, "52"

    res = b.TestMulticlassNamedArgs(
        myArg: Baml::Types::NamedArgsSingleClass.new(
            key: "key",
            key_two: true,
            key_three: 52,
        ),
        myArg2: Baml::Types::NamedArgsSingleClass.new(
            key: "key",
            key_two: true,
            key_three: 64,
        ),
    )
    assert_includes res, "52"
    assert_includes res, "64"

    res = b.TestFnNamedArgsSingleEnumList(myArg: [Baml::Types::NamedArgsSingleEnumList::TWO])
    assert_includes res, "TWO"

    res = b.TestFnNamedArgsSingleFloat(myFloat: 3.12)
    assert_includes res, "3.12"

    res = b.TestFnNamedArgsSingleInt(myInt: 3566)
    assert_includes res, "3566"

    res = b.TestFnNamedArgsSingleMapStringToString(myMap: {"lorem" => "ipsum"})
    assert_equal res['lorem'], "ipsum"

    res = b.TestFnNamedArgsSingleMapStringToClass(myMap: {"lorem" => {"word" => "ipsum"}})
    assert_equal res['lorem'].word, "ipsum"

    res = b.TestFnNamedArgsSingleMapStringToMap(myMap: {"lorem" => {"word" => "ipsum"}})
    assert_equal res['lorem']['word'], "ipsum"
  end

  it "accepts subclass of baml type" do
    # no-op- T::Struct cannot be subclassed
  end

  it "works with all outputs" do
    res = b.FnOutputBool(input: "a")
    assert_equal true, res

    list = b.FnOutputClassList(input: "a")
    assert list.size > 0
    assert list.first.prop1.size > 0

    classWEnum = b.FnOutputClassWithEnum(input: "a")
    assert classWEnum.prop2.serialize, "prop2"
    assert_includes ["ONE", "TWO"], classWEnum.prop2.serialize

    classs = b.FnOutputClass(input: "a")
    refute_nil classs.prop1
    assert_equal 540, classs.prop2

    enumList = b.FnEnumListOutput(input: "a")
    assert_equal 2, enumList.size

    myEnum = b.FnEnumOutput(input: "a")
    refute_nil myEnum
  end

  #it "should work with image" do
  #  res = b.TestImageInput(
  #    img: Baml::Image.from_url("https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png")
  #  )
  #  assert_includes res.downcase, "green"
  #end

  it "works with unions" do
    res = b.UnionTest_Function(input: "a")
    assert_includes res.inspect, "prop1"
    assert_includes res.inspect, "prop2"
    assert_includes res.inspect, "prop3"
  end

  it "works with retries" do
    assert_raises Exception do
      # calls a client that doesn't set api key correctly
      b.TestRetryExponential()
    end
  end

  it "works with fallbacks" do
    res = b.TestFallbackClient()
    assert res.size > 0
  end

  it "allows calling claude" do
    res = b.PromptTestClaude(input: "Mt Rainier is tall")
    assert res.size > 0
  end

  it "allows streaming" do
    stream = b.stream.PromptTestOpenAIChat(input: "Programming languages are fun to create")
    msgs = []
    stream.each do |msg|
      msgs << msg
    end
    final = stream.get_final_response

    assert final.size > 0, "Expected non-empty final but got empty."
    assert msgs.size > 0, "Expected at least one streamed response but got none."

    msgs.each_cons(2) do |prev_msg, msg|
      assert msg.start_with?(prev_msg), "Expected messages to be continuous, but prev was #{prev_msg} and next was #{msg}"
    end
    assert msgs.last == final, "Expected last stream message to match final response."
  end

  it "allows uniterated streaming" do
    final = b.stream.PromptTestOpenAIChat(input: "The color blue makes me sad").get_final_response
    assert final.size > 0, "Expected non-empty final but got empty."
  end

  it "allows streaming with claude" do
    stream = b.stream.PromptTestClaude(input: "Mt Rainier is tall")
    msgs = []
    stream.each do |msg|
      msgs << msg
    end
    final = stream.get_final_response

    assert final.size > 0, "Expected non-empty final but got empty."
    assert msgs.size > 0, "Expected at least one streamed response but got none."

    msgs.each_cons(2) do |prev_msg, msg|
      assert msg.start_with?(prev_msg), "Expected messages to be continuous, but prev was #{prev_msg} and next was #{msg}"
    end
    assert msgs.last == final, "Expected last stream message to match final response."
  end

  it "allows streaming of nested" do
    stream = b.stream.FnOutputClassNested(input: "a")
    msgs = []
    stream.each do |msg|
      puts msg
      msgs << msg
    end
    final = stream.get_final_response

    puts final
    assert msgs.size > 0, "Expected at least one streamed response but got none."
    assert msgs.last == final, "Expected last stream message to match final response."
  end

  it "tests dynamic" do
    tb = Baml::TypeBuilder.new
    tb.Person.add_property("last_name", tb.string.list)
    tb.Person.add_property("height", tb.float.optional).description("Height in meters")

    tb.Hobby.add_value("chess")
    tb.Hobby.list_values.each do |name, val|
      val.alias(name.downcase)
    end

    tb.Person.add_property("hobbies", tb.Hobby.type.list).description(
      "Some suggested hobbies they might be good at"
    )

    tb_res = b.ExtractPeople(
      "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.",
      {"tb" => tb}
    )

    refute_empty(tb_res, "Expected non-empty result but got empty.")

    tb_res.each do |r|
      puts r.model_dump
    end
  end

  it "tests dynamic class output" do
    tb = Baml::TypeBuilder.new
    tb.DynamicOutput.add_property("hair_color", tb.string)
    puts tb.DynamicOutput.list_properties
    tb.DynamicOutput.list_properties.each do |prop|
      puts "Property: #{prop}"
    end

    output = b.MyFunc(
      input: "My name is Harrison. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: tb} 
    )
    output = b.MyFunc(
      input: "My name is Harrison. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: tb} 
    )
    puts output.model_dump_json
    assert_equal("black", output.hair_color)
  end

  it "tests dynamic class nested output no stream" do
    tb = Baml::TypeBuilder.new
    nested_class = tb.add_class("Name")
    nested_class.add_property("first_name", tb.string)
    nested_class.add_property("last_name", tb.string.optional)
    nested_class.add_property("middle_name", tb.string.optional)

    other_nested_class = tb.add_class("Address")

    tb.DynamicOutput.add_property("name", nested_class.type.optional)
    tb.DynamicOutput.add_property("address", other_nested_class.type.optional)
    tb.DynamicOutput.add_property("hair_color", tb.string).alias("hairColor")
    tb.DynamicOutput.add_property("height", tb.float.optional)

    output = b.MyFunc(
      input: "My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: tb} 
    )
    puts output.model_dump_json
    assert_equal(
      '{"name":{"first_name":"Mark","last_name":"Gonzalez","middle_name":null},"address":null,"hair_color":"black","height":6.0}',
      output.model_dump_json
    )
  end

  it "tests dynamic class nested output stream" do
    tb = Baml::TypeBuilder.new
    nested_class = tb.add_class("Name")
    nested_class.add_property("first_name", tb.string)
    nested_class.add_property("last_name", tb.string.optional)

    tb.DynamicOutput.add_property("name", nested_class.type.optional)
    tb.DynamicOutput.add_property("hair_color", tb.string)

    stream = b.stream.MyFunc(
      input: "My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: tb} 
    )
    msgs = []
    stream.each do |msg|
      puts "streamed #{msg}"
      puts "streamed #{msg.model_dump}"
      msgs << msg
    end
    output = stream.get_final_response

    puts output.model_dump_json
    assert_equal(
      '{"name":{"first_name":"Mark","last_name":"Gonzalez"},"hair_color":"black"}',
      output.model_dump_json
    )
  end

  it "tests dynamic clients" do
    cb = Baml::Ffi::ClientRegistry.new
    cb.add_llm_client("MyClient", "openai", { model: "gpt-3.5-turbo" })
    cb.set_primary("MyClient")

    capitol = await BAML::ExpectFailure.new(
      baml_options: { client_registry: cb }
    )
    assert_match(/london/, capitol.downcase)
  end
end
