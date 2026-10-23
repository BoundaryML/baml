# typed: strict
require "sorbet-runtime"
require_relative "baml_sdk"

T.let(BamlSdk::Primitives.return_bigint, Integer)
T.let(BamlSdk::Primitives.round_trip_float(7), Float)
T.let(BamlSdk::Primitives.round_trip_uint8_array("bytes".b), String)
T.let(BamlSdk::Aliases.round_trip_string_list(["a"]), BamlSdk::Aliases::StringList)
T.let(BamlSdk::Aliases.round_trip_rec_list([1, [2]]), BamlSdk::Aliases::RecList)
T.let(BamlSdk::Unions.round_trip_dedup("s"), T.any(Integer, String))
T.let(BamlSdk::Enums.pick_positive, BamlSdk::Enums::Sentiment)
image = Baml::Image.from_url("https://example.com/image.png")
T.let(BamlSdk::Media.round_trip_image(image), Baml::Image)
T.let(BamlSdk::Void.no_op, NilClass)
node = BamlSdk::Recursion::IntBinaryTree.new(value: 1, left: nil, right: nil)
T.let(BamlSdk::Recursion.round_trip_int_binary_tree(node), BamlSdk::Recursion::IntBinaryTree)
