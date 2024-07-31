generator lang_python_1 {
  output_type python/pydantic
  output_dir "../"
}

generator lang_python_2 {
  output_type python/pydantic
  default_client_mode "async"
  output_dir "../"
}

generator lang_python_3 {
  output_type python/pydantic
  default_client_mode "sync"
  output_dir "../"
}

generator lang_typescript_1 {
  output_type typescript
  default_client_mode "async"
  output_dir "../"
}


generator lang_typescript_2 {
  output_type typescript
  default_client_mode "sync"
  output_dir "../"
}

generator lang_ruby_1 {
  output_type typescript
  default_client_mode "async"
  output_dir "../"
}

generator lang_ruby_2 {
  output_type typescript
  default_client_mode "sync"
  output_dir "../"
}

// error: Error validating function "lang_python_1": Invalid function syntax. Valid function syntax is
// ```
// function lang_python_1(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:1
//    | 
//    | 
//  1 | generator lang_python_1 {
//  2 |   output_type python/pydantic
//  3 |   output_dir "../"
//  4 | }
//    | 
// error: Error validating function "lang_python_2": Invalid function syntax. Valid function syntax is
// ```
// function lang_python_2(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:6
//    | 
//  5 | 
//  6 | generator lang_python_2 {
//  7 |   output_type python/pydantic
//  8 |   default_client_mode "async"
//  9 |   output_dir "../"
// 10 | }
//    | 
// error: Error validating function "lang_python_3": Invalid function syntax. Valid function syntax is
// ```
// function lang_python_3(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:12
//    | 
// 11 | 
// 12 | generator lang_python_3 {
// 13 |   output_type python/pydantic
// 14 |   default_client_mode "sync"
// 15 |   output_dir "../"
// 16 | }
//    | 
// error: Error validating function "lang_typescript_1": Invalid function syntax. Valid function syntax is
// ```
// function lang_typescript_1(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:18
//    | 
// 17 | 
// 18 | generator lang_typescript_1 {
// 19 |   output_type typescript
// 20 |   default_client_mode "async"
// 21 |   output_dir "../"
// 22 | }
//    | 
// error: Error validating function "lang_typescript_2": Invalid function syntax. Valid function syntax is
// ```
// function lang_typescript_2(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:25
//    | 
// 24 | 
// 25 | generator lang_typescript_2 {
// 26 |   output_type typescript
// 27 |   default_client_mode "sync"
// 28 |   output_dir "../"
// 29 | }
//    | 
// error: Error validating function "lang_ruby_1": Invalid function syntax. Valid function syntax is
// ```
// function lang_ruby_1(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:31
//    | 
// 30 | 
// 31 | generator lang_ruby_1 {
// 32 |   output_type typescript
// 33 |   default_client_mode "async"
// 34 |   output_dir "../"
// 35 | }
//    | 
// error: Error validating function "lang_ruby_2": Invalid function syntax. Valid function syntax is
// ```
// function lang_ruby_2(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  generators/v1.baml:37
//    | 
// 36 | 
// 37 | generator lang_ruby_2 {
// 38 |   output_type typescript
// 39 |   default_client_mode "sync"
// 40 |   output_dir "../"
// 41 | }
//    | 
