type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

class RecursiveAliasDependency {
  value JsonValue
}

function FnOutputTypeAliasJsonDep(input: RecursiveAliasDependency) -> RecursiveAliasDependency {
    client "openai/gpt-4o"
    prompt #"
      Return the given input back:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasJsonDep {
  functions [FnOutputTypeAliasJsonDep]
  args {
    input {
      value "hello"
    }
  }
}
