class Named { value: int @alias("n") }
class Other { value: int }
class Empty {}
class Defaults {
  n: int @skip
  s: string @skip
  op: "b" | "a" @skip
  optional: int? @skip
}
function named(naming: baml.json.Naming) -> int {
  let v = baml.json.decode<Named>("{\"value\":3,\"n\":5}", naming=naming);
  v.value
}
function main() -> int {
  let other = baml.json.decode<Other>("{\"value\":7}");
  let defaults = baml.json.decode<Defaults>("{\"n\":99,\"s\":123}", naming=baml.json.Naming.Aliased);
  let union = baml.json.decode<Empty | Named>("{\"n\":11}", naming=baml.json.Naming.Aliased);
  let selected = match(union) { Empty => 0, Named => union.value };
  let literal = match(defaults.op) { "b" => 100, "a" => 200 };
  named(baml.json.Naming.Declared) + named(baml.json.Naming.Aliased) + other.value + defaults.n + defaults.s.length() + (defaults.optional ?? 13) + selected + literal
}
