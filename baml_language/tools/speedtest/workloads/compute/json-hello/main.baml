// JSON hello world (from the native-bench json_hello case): serialize one
// small record. The job is so small that per-call overhead dominates.
class Hello {
  message: string
}

function setup() -> Hello {
  Hello { message: "Hello, World!" }
}

function run(hello: Hello) -> string {
  baml.json.to_string(hello)
}

function main() -> string {
  run(setup())
}
