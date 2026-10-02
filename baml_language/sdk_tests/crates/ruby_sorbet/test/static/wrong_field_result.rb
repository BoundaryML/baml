# typed: strict
require_relative "../../function_calls/generated/baml_sdk"

person = BamlSdk::Person.new(person: "person", name: "Ryan", age: 30)
T.let(person.age, String) # error: 7007
