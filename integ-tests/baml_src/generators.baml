generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version "0.58.0"
}


generator openapi {
  output_type rest/openapi
  output_dir "../openapi"
  version "0.58.0"
  on_generate "rm .gitignore"
}
