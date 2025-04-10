generator lang_python {
  output_type python/pydantic
  output_dir "../python"
  version "0.68.0"
}

class ETA {
    thing string
}

class Foo {
    if string
    ETA ETA?
}

// error: Error validating field `if` in class `if`: Field name is a reserved word in generated python/pydantic clients.
//   -->  class/generator_keywords1.baml:12
//    | 
// 11 | class Foo {
// 12 |     if string
// 13 |     ETA ETA?
//    | 
// error: Error validating field `ETA` in class `ETA`: When using the python/pydantic generator, a field name must not be exactly equal to the type name. Consider changing the field name and using an alias.
//   -->  class/generator_keywords1.baml:13
//    | 
// 12 |     if string
// 13 |     ETA ETA?
// 14 | }
//    | 
