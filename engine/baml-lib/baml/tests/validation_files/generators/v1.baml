generator lang_python_1 {
  output_type python/pydantic
  output_dir "../"
}

// error: Error validating: Missing 'version' property in your generator. It must match the baml package version you have installed in your project and the VSCode extension, like 'version "0.50.0"'
//   -->  generators/v1.baml:1
//    | 
//    | 
//  1 | generator lang_python_1 {
//  2 |   output_type python/pydantic
//  3 |   output_dir "../"
//  4 | }
//    | 
