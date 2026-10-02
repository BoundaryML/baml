# typed: strict
require_relative "../../function_calls/generated/baml_sdk"

BamlSdk::Person.new(person: "person", name: "Ryan", age: "30") # error: 7002
