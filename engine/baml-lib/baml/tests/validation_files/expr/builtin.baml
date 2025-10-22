class Todo {
  id int
  todo string
  completed bool
  userId int
}

function GetTodo() -> Todo {
<<<<<<< HEAD
  baml.fetch_as<Todo>(baml.HttpRequest {
    method: baml.HttpMethod.Get,
    url: "https://dummyjson.com/todos/1",
=======
  baml.fetch_value<Todo>(baml.Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
>>>>>>> canary
  })
}

function GetTodoMissingTypeArg() -> Todo {
<<<<<<< HEAD
  baml.fetch_as(baml.HttpRequest {
    method: baml.HttpMethod.Get,
    url: "https://dummyjson.com/todos/1",
  })
}

// error: Error validating: could not infer return type of baml.fetch_as
//   -->  expr/builtin.baml:16
//    | 
// 15 | function GetTodoMissingTypeArg() -> Todo {
// 16 |   baml.fetch_as(baml.HttpRequest {
// 17 |     method: baml.HttpMethod.Get,
// 18 |     url: "https://dummyjson.com/todos/1",
// 19 |   })
=======
  baml.fetch_value(baml.Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

// error: Error validating: Generic function baml.fetch_value must have a type argument. Try adding a type argument like this: baml.fetch_value<Type>
//   -->  expr/builtin.baml:17
//    | 
// 16 | function GetTodoMissingTypeArg() -> Todo {
// 17 |   baml.fetch_value(baml.Request {
// 18 |     base_url: "https://dummyjson.com/todos/1",
// 19 |     headers: {},
// 20 |     query_params: {},
// 21 |   })
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  expr/builtin.baml:8
//    | 
//  7 | 
//  8 | function GetTodo() -> Todo {
//  9 | <<<<<<< HEAD
//    | 
