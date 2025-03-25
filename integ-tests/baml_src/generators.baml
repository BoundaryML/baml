generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version "0.81.1"
}

generator lang_typescript {
  output_type typescript
  output_dir "../typescript"
  version "0.81.1"
}

generator lang_typescript_react {
  output_type typescript/react
  output_dir "../react"
  version "0.81.1"
}

generator lang_ruby {
  output_type ruby/sorbet
  output_dir "../ruby"
  version "0.81.1"
}

generator openapi {
  output_type rest/openapi
  output_dir "../openapi"
  version "0.81.1"
  on_generate "rm .gitignore"
}