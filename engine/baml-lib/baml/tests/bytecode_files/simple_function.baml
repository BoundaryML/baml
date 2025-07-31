function GetName(person: string) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    Extract the name from: {{ person }}
  "#
}

// Bytecode tests require baml_compiler and baml_vm dependencies