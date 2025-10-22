class Todo {
  id int
  todo string
  completed bool
  userId int
}

function GetTodo() -> Todo {
<<<<<<< HEAD
  std.fetch_value<Todo>(std.Request {
=======
  baml.fetch_value<Todo>(baml.Request {
>>>>>>> canary
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

function GetTodoMissingTypeArg() -> Todo {
<<<<<<< HEAD
  std.fetch_value(std.Request {
=======
  baml.fetch_value(baml.Request {
>>>>>>> canary
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

<<<<<<< HEAD
// error: Error validating: Generic function std.fetch_value must have a type argument. Try adding a type argument like this: std.fetch_value<Type>
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   std.fetch_value(std.Request {
=======
// error: Error validating: Generic function baml.fetch_value must have a type argument. Try adding a type argument like this: baml.fetch_value<Type>
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   baml.fetch_value(baml.Request {
>>>>>>> canary
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
// 20 |     query_params: {},
// 21 |   })
//    | 
