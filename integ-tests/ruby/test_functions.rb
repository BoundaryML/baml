require 'json'
require 'minitest/autorun'
require 'minitest/reporters'

require_relative "baml_client/client"


b = Baml.Client

describe "ruby<->baml integration tests" do
  it "works with all inputs" do
    res = b.TestFnNamedArgsSingleBool(myBool: true)
    assert_equal res, "true"

    res = b.TestNamedArgsLiteralInt(myInt: 1)
    assert_includes res, "1"

    res = b.TestNamedArgsLiteralBool(myBool: true)
    assert_includes res, "true"

    res = b.TestNamedArgsLiteralString(myString: "My String")
    assert_includes res, "My String"

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

    res = b.InOutEnumMapKey(i1: {"A" => "A"}, i2: {"B" => "B"})
    assert_equal res['A'], "A"
    assert_equal res['B'], "B"

    res = b.InOutLiteralStringUnionMapKey(i1: {"one" => "1"}, i2: {"two" => "2"})
    assert_equal res['one'], "1"
    assert_equal res['two'], "2"

    res = b.InOutSingleLiteralStringMapKey(m: {"key" => "1"})
    assert_equal res['key'], "1"

    res = b.PrimitiveAlias(p: "test")
    assert_equal res, "test"

    res = b.MapAlias(m: {"A" => ["B", "C"], "B" => [], "C" => []})
    assert_equal res, {"A" => ["B", "C"], "B" => [], "C" => []}

    res = b.NestedAlias(c: "test")
    assert_equal res, "test"

    res = b.NestedAlias(c: {"A" => ["B", "C"], "B" => [], "C" => []})
    assert_equal res, {"A" => ["B", "C"], "B" => [], "C" => []}

    res = b.AliasThatPointsToRecursiveType(list: Baml::Types::LinkedListAliasNode.new(
        value: 1,
        next: nil,
    ))
    # TODO: Doesn't implement equality
    # assert_equal res, Baml::Types::LinkedListAliasNode.new(
    #   value: 1,
    #   next: nil,
    # )
    
    res = b.ClassThatPointsToRecursiveClassThroughAlias(
        cls: Baml::Types::ClassToRecAlias.new(
            list: Baml::Types::LinkedListAliasNode.new(
                value: 1,
                next: nil,
            )
        )
    )

    res = b.RecursiveClassWithAliasIndirection(
        cls: Baml::Types::NodeWithAliasIndirection.new(
            value: 1,
            next: Baml::Types::NodeWithAliasIndirection.new(
                value: 2,
                next: nil,
            )
        )
    )
  end

  it "optional map and list" do
    res = b.AllowedOptionals(optionals: Baml::Types::OptionalListAndMap.new(
      p: nil,
      q: nil,
    ))
    assert_equal res.p, nil
    assert_equal res.q, nil

    res = b.AllowedOptionals(optionals: Baml::Types::OptionalListAndMap.new(
      p: ["test"],
      q: {"test" => "ok"},
    ))
    assert_equal res.p, ["test"]
    assert_equal res.q, {"test" => "ok"}
  end

  it "accepts subclass of baml type" do
    # no-op- T::Struct cannot be subclassed
  end

  it "works with all outputs" do
    res = b.FnOutputBool(input: "a")
    assert_equal true, res

    integer = b.FnOutputInt(input: "a")
    assert_equal integer, 5

    literal_integer = b.FnOutputLiteralInt(input: "a")
    assert_equal literal_integer, 5
    
    literal_bool = b.FnOutputLiteralBool(input: "a")
    assert_equal literal_bool, false

    literal_string = b.FnOutputLiteralString(input: "a")
    assert_equal literal_string, "example output"

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

  it "should work with image" do
   res = b.TestImageInput(
     img: Baml::Image.from_url("https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png")
   )
   assert_includes res.downcase, "green"
  end

  it "should work with audio" do
    # from URL
    res = b.AudioInput(
      aud: Baml::Audio.from_url("https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg")
    )
  end

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
    puts "TEST"
    stream.each do |msg|
      print("INNER")
      msgs << msg
    end
    final = stream.get_final_response

    puts msgs.last.to_json
    puts final.to_json
    assert msgs.size > 0, "Expected at least one streamed response but got none."
    assert msgs.last.to_json == final.to_json, "Expected last stream message to match final response."
  end

  it "tests dynamic" do
    t = Baml::TypeBuilder.new
    t.Person.add_property("last_name", t.string.list)
    t.Person.add_property("height", t.float.optional).description("Height in meters")

    t.Hobby.add_value("chess")
    # TODO: figure out a non-naive impl of #list_values in Ruby
    # t.Hobby.list_values.each do |name, val|
    #   val.alias(name.downcase)
    # end

    t.Person.add_property("hobbies", t.Hobby.type.list).description(
      "Some suggested hobbies they might be good at"
    )

    t_res = b.ExtractPeople(
      text: "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good on a chessboard.",
      baml_options: {tb: t}
    )

    refute_empty(t_res, "Expected non-empty result but got empty.")

    t_res.each do |r|
      puts r.inspect
      assert_kind_of(Float, r['height'])
      assert_kind_of(Float, r[:height])
    end
  end

  it "tests dynamic class output" do
    t = Baml::TypeBuilder.new
    t.DynamicOutput.add_property("hair_color", t.string)
    # TODO: figure out a non-naive impl of #list_properties in Ruby
    # puts t.DynamicOutput.list_properties
    # t.DynamicOutput.list_properties.each do |prop|
    #   puts "Property: #{prop}"
    # end

    output = b.MyFunc(
      input: "My name is Harrison. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: t} 
    )
    puts output.inspect
    assert_equal("black", output.hair_color)
  end

  it "tests dynamic class nested output no stream" do
    t = Baml::TypeBuilder.new
    nested_class = t.add_class("Name")
    nested_class.add_property("first_name", t.string)
    nested_class.add_property("last_name", t.string.optional)
    nested_class.add_property("middle_name", t.string.optional)

    other_nested_class = t.add_class("Address")

    t.DynamicOutput.add_property("name", nested_class.type.optional)
    t.DynamicOutput.add_property("address", other_nested_class.type.optional)
    t.DynamicOutput.add_property("hair_color", t.string).alias("hairColor")
    t.DynamicOutput.add_property("height", t.float.optional)

    output = b.MyFunc(
      input: "My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: t} 
    )
    puts output.inspect
    assert_equal(
      '{"name":{"first_name":"Mark","last_name":"Gonzalez","middle_name":null},"address":null,"hair_color":"black","height":6.0}',
      output.to_json
    )
  end

  it "tests dynamic class nested output stream" do
    t = Baml::TypeBuilder.new
    nested_class = t.add_class("Name")
    nested_class.add_property("first_name", t.string)
    nested_class.add_property("last_name", t.string.optional)

    t.DynamicOutput.add_property("name", nested_class.type.optional)
    t.DynamicOutput.add_property("hair_color", t.string)

    stream = b.stream.MyFunc(
      input: "My name is Mark Gonzalez. My hair is black and I'm 6 feet tall.",
      baml_options: {tb: t} 
    )
    msgs = []
    stream.each do |msg|
      puts "streamed #{msg}"
      puts "streamed #{msg.model_dump}"
      msgs << msg
    end
    output = stream.get_final_response

    puts output.inspect
    assert_equal(
      '{"name":{"first_name":"Mark","last_name":"Gonzalez"},"hair_color":"black"}',
      output.to_json
    )
  end

  it "tests dynamic clients" do
    cb = Baml::Ffi::ClientRegistry.new
    cb.add_llm_client("MyClient", "openai", { model: "gpt-3.5-turbo" })
    cb.set_primary("MyClient")

    capitol = b.ExpectFailure(
      baml_options: { client_registry: cb }
    )
    assert_match(/london/, capitol.downcase)
  end

  it "uses constraints for unions" do
    res = b.ExtractContactInfo(document: "reach me at 888-888-8888, or try to email hello@boundaryml.com")
    assert_equal res['primary']['value'], "888-888-8888"
  end

  it "uses constraints" do
    res = b.PredictAge(name: "Greg")
    assert_equal res["certainty"].checks[:unreasonably_certain].status, "failed"
  end

  it "uses block_level constraints" do
    res = b.MakeBlockConstraint()
    assert_equal res.checks[:cross_field].status, "failed"
  end

  it "uses nested_block_level constraints" do
      res = b.MakeNestedBlockConstraint()
      assert_equal res["nbc"].checks[:cross_field].status, "succeeded"
  end

  it "uses block_level constraints in function parameters" do
    block_constraint = Baml::Types::BlockConstraintForParam.new(bcfp: 1, bcfp2: "too long!")
    assert_raises Exception do
      res = b.UseBlockConstraint(inp: block_constraint)
    end
    nested_block_constraint = Baml::Types::NestedBlockConstraintForParam.new(
      nbcfp: block_constraint
    )
    assert_raises Exception do
      res = b.UseNestedBlockConstraint(inp: nested_block_constraint)
    end
  end

  it "uses semantic_container" do
    stream = b.stream.MakeSemanticContainer()
    stream.each do |msg|
      puts msg.to_json
    end
  end

  it "uses semantic_streaming" do
    stream = b.stream.MakeSemanticContainer()

    reference_string = nil
    reference_int = nil

    msgs = []
    puts "HELLO'"

    stream.each do |msg|
      puts "THERE"
      puts msg.to_json

      msgs << msg

       # Check value stability.
      if !msg.sixteen_digit_number.nil?
        if reference_int.nil?
          reference_int = msg.sixteen_digit_number
        else
          assert_equal reference_int, msg.sixteen_digit_number
        end
      end
      if !msg.string_with_twenty_words.nil?
        if reference_string.nil?
          reference_string = msg.string_with_twenty_words
        else
          assert_equal reference_string, msg.string_with_twenty_words
        end
      end

      # Check for @stream.with_state.
      if !msg.class_needed.nil?
        if !msg.class_needed.s_20_words.value.nil?
          if len(msg.class_needed.s_20_words.value.split(" ")) < 3 && msg.final_string.nil?
            puts(msg)
            assert msg.class_needed.s_20_words.state == "Incomplete"
          end
        end
      end
      if !msg.final_string.nil?
        assert msg.class_needed.s_20_words.state == "Complete"
      end
    end

    puts "TRY FINAL"
    final = stream.get_final_response
    puts final.to_json
  end

end
