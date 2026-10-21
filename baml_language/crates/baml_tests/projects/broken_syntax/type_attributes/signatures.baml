// Only fields, enum variants, and config items take trailing attributes. Type
// alias bodies, parameters, and return types are types, which take none.

class EFoo { x int }
class EA { x int }
class EB { x int }

type AliasWithAttribute = EFoo @alias("alias")

type AliasWithStreamAttribute = EFoo @stream.done

type AliasWithBothAttributes = EFoo @alias("alias") @stream.done

type UnionAliasWithAttribute = EA | EB @alias("alias")

function param_with_attribute(x: EFoo @alias("param")) -> string {
  "hello"
}

function param_with_stream_attribute(x: EFoo @stream.done) -> string {
  "hello"
}

function return_with_attribute() -> EFoo @alias("return") {
  EFoo { x: 1 }
}

function return_with_stream_attribute() -> EFoo @stream.done {
  EFoo { x: 1 }
}
