# typed: strict
require_relative "../../function_calls/generated/baml_sdk"

T.let(BamlSdk.single_required_arg("hello"), Integer) # error: 7007
