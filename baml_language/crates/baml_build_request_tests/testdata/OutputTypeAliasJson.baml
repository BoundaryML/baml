type JsonValue = int | string | bool | float | JsonObject | JsonArray
type JsonObject = map<string, JsonValue>
type JsonArray = JsonValue[]

function FnOutputTypeAliasJson(input: JsonValue) -> JsonValue {
    client "openai/gpt-4o"
    prompt #"
      Return the given input back:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

test TestOutputTypeAliasJson {
  functions [FnOutputTypeAliasJson]
  args {
    input "hello"
  }
}
