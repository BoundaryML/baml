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
T.let(BamlSdk::StaticMethodEdges::Edge.nullable(nil), T.nilable(Integer))
