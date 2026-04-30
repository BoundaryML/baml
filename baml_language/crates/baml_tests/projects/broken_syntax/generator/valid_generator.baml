// Test: Generator declarations are properly parsed

generator lang_python {
  output_type "python/pydantic"
  output_dir "../python"
  version "0.68.0"
}

generator lang_typescript {
  output_type typescript
  output_dir "../typescript"
  default_client_mode "async"
}

class MyClass {
    name string
    age int
}

function GetUser() -> MyClass {
    client<llm> MyClient
    prompt #"Hello"#
}
