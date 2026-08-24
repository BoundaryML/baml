client HostReflectParser = openai.OpenAiClient.new(
  model = "gpt-4o-mini",
  api_key = "not-used-by-parse-companions",
);

class StaticRecord {
  name string
}

enum StaticChoice {
  ONE
  TWO
}

interface StaticNamed {
  name string
}

// Makes the erased interface token part of the generated public host API.
function HostReadNamed(value: StaticNamed) -> string {
  value.name
}

function HostExtract<T>(input: string) -> T {
  client HostReflectParser
  prompt `Extract a value from ${input}.\n${ctx.output_format}`
}

// Pure, offline entry point used by the Python host-boundary tests.
function HostParse<T>(raw: string) -> T {
  HostExtract$parse<T>(raw)
}

function HostTypeEqual<A, B>() -> bool {
  reflect.Type.of<A>() == reflect.Type.of<B>()
}

function HostTypeName<T>() -> string {
  reflect.Type.of<T>().to_string()
}
