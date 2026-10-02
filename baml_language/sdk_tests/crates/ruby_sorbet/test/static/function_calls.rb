# typed: strict
require "sorbet-runtime"
require_relative "../../function_calls/generated/baml_sdk"

T.let(BamlSdk.initialize!, NilClass)
T.let(BamlSdk.hello_world, String)
T.let(BamlSdk.single_required_arg("hello"), String)
person = BamlSdk::Person.new(person: "person", name: "Ryan", age: 30)
T.let(person.person, String)
T.let(person.name, String)
T.let(person.age, Integer)
result = T.let(BamlSdk.round_trip_person(person), BamlSdk::Person)
T.let(result.name, String)
T.let(result.age, Integer)
# These supplied defaults are statically supported; omission still raises.
T.let(BamlSdk.optional_args_probe(1, opt1: nil, opt2: 2), T::Array[T.nilable(Integer)])
T.let(BamlSdk.default_name_collisions("a", options: "b", option: "c"), T::Array[String])
