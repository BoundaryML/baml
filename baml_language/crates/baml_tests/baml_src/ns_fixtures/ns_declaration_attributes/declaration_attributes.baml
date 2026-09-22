// Attributes trail the declaration they annotate: a class field, an enum
// variant, or (with `@@`) the class or enum itself. Any number of attributes
// may follow a field of any type shape, in any order.

class Fizz {
  x int
}

class UA { x int }
class UB { x int }
class UC { x int }

class AttributeOrdering {
  stream_only Fizz @stream.done
  alias_only Fizz @alias("alias_only_name")
  stream_then_alias Fizz @stream.done @alias("stream_then_alias_name")
  alias_then_stream Fizz @alias("alias_then_stream_name") @stream.done
  interleaved Fizz @alias("interleaved_name") @stream.done @description("interleaved")
  stream_pair Fizz @stream.done @stream.must_exist
  split_stream_pair Fizz @stream.done @alias("split_stream_pair_name") @stream.must_exist
  plain_pair Fizz @alias("plain_pair_name") @description("plain pair")
}

class AttributesAfterTypeShapes {
  union_stream UA | UB | UC @stream.done
  union_alias UA | UB | UC @alias("union_alias_name")
  union_mixed UA | UB | UC @alias("union_mixed_name") @stream.done
  parenthesized_union (UA | UB) @stream.done
  parenthesized_union_mixed (UA | UB) @alias("parenthesized_union_mixed_name") @stream.done
  parenthesized_union_list (UA | UB)[] @alias("parenthesized_union_list_name")
  optional_class Fizz? @stream.done
  class_list Fizz[] @alias("class_list_name") @stream.done
  optional_class_list Fizz[]? @alias("optional_class_list_name") @stream.done
  class_map map<string, Fizz> @alias("class_map_name") @stream.done
}

enum Priority {
  Critical @alias("critical") @description("drop everything")
  High @alias("high")
  Normal @description("the default")
  Retired @skip
  @@description("Incident priority levels")
}

enum Size {
  Small
  Medium
  Large
  @@description("T-shirt sizes")
  @@alias("TShirtSize")
}

class Point {
  x float
  y float
  @@stream.done
}

class ConfigEntry {
  key string
  value string
  @@stream.done
  @@description("Configuration entry")
}

class References {
  priority Priority @alias("p")
  size Size
  point Point @alias("pt")
  points Point[]
  maybe_point Point?
  entries map<string, ConfigEntry>
}
