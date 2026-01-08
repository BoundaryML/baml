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
//    | 
// error: Error validating field `ETA` in class `ETA`: When using the python/pydantic generator, a field name must not be exactly equal to the type name. Consider changing the field name and using an alias.
//   -->  class/generator_keywords1.baml:13
//    | 
// 12 |     if string
// 13 |     ETA ETA?
//    |

//----
//- diagnostics
// Error: Expected Unexpected token in class body, found if
//     ╭─[ class_generator_keywords1.baml:12:5 ]
//     │
//  12 │     if string
//     │     ─┬  
//     │      ╰── Expected Unexpected token in class body, found if
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: field 'string' is missing a type annotation
//     ╭─[ class_generator_keywords1.baml:12:8 ]
//     │
//  12 │     if string
//     │        ───┬──  
//     │           ╰──── field 'string' is missing a type annotation
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Field `ETA` in class `Foo` has the same name as its type `ETA`, which is not supported in generated Python code.
//     ╭─[ class_generator_keywords1.baml:13:5 ]
//     │
//  13 │     ETA ETA?
//     │     ─┬─  
//     │      ╰─── Field `ETA` in class `Foo` has the same name as its type `ETA`, which is not supported in generated Python code.
//     │ 
//     │ Note: Error code: E0021
// ────╯
