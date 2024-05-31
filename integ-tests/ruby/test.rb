require_relative "baml_client/client"

b = Baml::BamlClient.from_directory("integ-tests/baml_src")

puts "Example 0: round-tripping data (nullable is object)"
input = Baml::Types::RoundtripObject.new(
        my_int: 4,
        my_float: 3.14,
        my_string: "hello",
        greek_letter: Baml::Types::GreekLetter::ALPHA,
        nullable: Baml::Types::FizzBuzz.new(beer: "IPA", wine: "merlot"),
        string_list: ["a", "b", "c"],
        primitive_union: 4,
        object_union: [Baml::Types::ChoiceBar.new(
                bar: "hello",
                hebrew_letter: Baml::Types::HebrewLetter::ALEPH,
        )]
)
output = b.RoundtripMyData(input: input)
pp output
puts

puts "Example 1: round-tripping data (nullable is string)"
input = Baml::Types::RoundtripObject.new(
        my_int: 4,
        my_float: 3.14,
        my_string: "hello",
        greek_letter: Baml::Types::GreekLetter::ALPHA,
        #nullable: Baml::Types::FizzBuzz.new(beer: "IPA", wine: "merlot"),
        nullable: "hello",
        string_list: ["a", "b", "c"],
        primitive_union: 4,
        object_union: [Baml::Types::ChoiceBar.new(
                bar: "hello",
                hebrew_letter: Baml::Types::HebrewLetter::ALEPH,
        )]
)
output = b.RoundtripMyData(input: input)
pp output
puts

puts "Example 2: round-tripping data (nullable is null)"
input = Baml::Types::RoundtripObject.new(
        my_int: 4,
        my_float: 3.14,
        my_string: "hello",
        greek_letter: Baml::Types::GreekLetter::ALPHA,
        nullable: nil,
        string_list: ["a", "b", "c"],
        primitive_union: 4,
        object_union: [Baml::Types::ChoiceBar.new(
                bar: "hello",
                hebrew_letter: Baml::Types::HebrewLetter::ALEPH,
        )]
)
output = b.RoundtripMyData(input: input)
pp output
puts