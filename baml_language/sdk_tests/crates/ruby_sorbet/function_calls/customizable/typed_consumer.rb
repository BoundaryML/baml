# typed: strict
require "sorbet-runtime"
require_relative "baml_sdk"

T.let(BamlSdk.initialize!, NilClass)
T.let(BamlSdk.hello_world, String)
T.let(BamlSdk.hello_world_async, Baml::Call[String])
T.let(BamlSdk::Baml::Float.parse("1.5"), Float)
T.let(BamlSdk::Baml::String.from_code_points([65]), String)
T.let(BamlSdk::Baml::Spawn::CancelToken.baml_new_async, Baml::Call[BamlSdk::Baml::Spawn::CancelToken])
T.let(BamlSdk.single_required_arg("hello"), String)
person = BamlSdk::Person.new(person: "person", name: "Ryan", age: 30)
T.let(person.person, String)
T.let(person.name, String)
T.let(person.age, Integer)
result = T.let(BamlSdk.round_trip_person(person), BamlSdk::Person)
T.let(result.name, String)
T.let(result.age, Integer)
# Omission, UNSET and nil remain distinct, with typed per-call controls.
T.let(BamlSdk.optional_args_probe(1), T::Array[T.nilable(Integer)])
T.let(BamlSdk.optional_args_probe(1, opt1: Baml::UNSET, opt2: nil), T::Array[T.nilable(Integer)])
T.let(BamlSdk.optional_args_probe(1, opt1: nil, opt2: 2, _baml: Baml::Options.new(timeout_ms: 1000)), T::Array[T.nilable(Integer)])
T.let(BamlSdk::StaticMethodEdges::Edge.nullable(nil), T.nilable(Integer))
T.let(BamlSdk.default_name_collisions("a", options: "b", option: "c"), T::Array[String])
