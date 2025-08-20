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

// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/builtin.baml:8
//    | 
//  7 | 
//  8 | function GetTodo() -> Todo {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/builtin.baml:16
//    | 
// 15 | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
//    | 
// error: Generic function std::fetch_value must have a type argument. Try adding a type argument like this: std::fetch_value<Type>
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   std::fetch_value(std::Request {
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
// 20 |     query_params: {},
// 21 |   })
//    | 
// error: Expected a class value, but received string value `std::fetch_value<Todo>(std::Request { base_url: "https://dummyjson.com/todos/1", headers: {}, query_params: {} })`.
//   -->  expr/builtin.baml:9
//    | 
//  8 | function GetTodo() -> Todo {
//  9 |   std::fetch_value<Todo>(std::Request {
// 10 |     base_url: "https://dummyjson.com/todos/1",
// 11 |     headers: {},
// 12 |     query_params: {},
// 13 |   })
//    | 
// error: Error validating: Generic function std::fetch_value must have a type argument. Try adding a type argument like this: std::fetch_value<Type>
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   std::fetch_value(std::Request {
//    | 
// error: Expected a class value, but received string value `std::fetch_value(std::Request { base_url: "https://dummyjson.com/todos/1", headers: {}, query_params: {} })`.
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   std::fetch_value(std::Request {
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
// 20 |     query_params: {},
// 21 |   })
//    | 
