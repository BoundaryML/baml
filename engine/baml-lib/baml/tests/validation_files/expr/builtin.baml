class Todo {
  id int
  todo string
  completed bool
  userId int
}

function GetTodo() -> Todo {
  std::fetch_value<Todo>(std::Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

function GetTodoMissingTypeArg() -> Todo {
  std::fetch_value(std::Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

// error: Error validating Function "GetTodoMissingTypeArg": This field declaration is invalid. It is either missing a name or a type.
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   std::fetch_value(std::Request {
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   std::fetch_value(std::Request {
// 18 |     base_url: "https://dummyjson.com/todos/1",
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  expr/builtin.baml:18
//    | 
// 17 |   std::fetch_value(std::Request {
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  expr/builtin.baml:19
//    | 
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
// 20 |     query_params: {},
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  expr/builtin.baml:20
//    | 
// 19 |     headers: {},
// 20 |     query_params: {},
// 21 |   })
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  expr/builtin.baml:21
//    | 
// 20 |     query_params: {},
// 21 |   })
// 22 | }
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  expr/builtin.baml:22
//    | 
// 21 |   })
// 22 | }
// 23 | 
//    | 
// error: Error validating: Unknown field `base_url` in function
//   -->  expr/builtin.baml:18
//    | 
// 17 |   std::fetch_value(std::Request {
// 18 |     base_url: "https://dummyjson.com/todos/1",
//    | 
// error: Error validating: Unknown field `headers` in function
//   -->  expr/builtin.baml:19
//    | 
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
//    | 
// error: Error validating: Unknown field `query_params` in function
//   -->  expr/builtin.baml:20
//    | 
// 19 |     headers: {},
// 20 |     query_params: {},
//    | 
// error: Error validating: Missing `prompt` and `client` fields in function. Add to the block:
// ```
// client GPT4
// prompt #"..."#
// ```
//   -->  expr/builtin.baml:16
//    | 
// 15 | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
//    | 
