generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version "0.212.0"
}

generator lang_python_v1 {
  output_type python/pydantic/v1
  output_dir "../python-v1"
  version "0.212.0"
}

generator lang_typescript {
  output_type typescript
  output_dir "../typescript"
  version "0.212.0"
}
 

generator lang_typescript_esm {
  output_type typescript
  output_dir "../typescript-esm"
  version "0.212.0"
  module_format esm
}


generator lang_typescript_react {
  output_type typescript/react
  output_dir "../react"
  version "0.212.0"
}

generator lang_ruby {
  output_type ruby/sorbet
  output_dir "../ruby"
  version "0.212.0"
}

generator openapi {
  output_type rest/openapi
  output_dir "../openapi"
  version "0.212.0"
  on_generate "rm .gitignore"
}

generator lang_go {
  output_type go
  output_dir "../go"
  version "0.212.0"
  client_package_name "example.com/integ-tests"
  on_generate "gofmt -w . && goimports -w . && go mod tidy"
}
