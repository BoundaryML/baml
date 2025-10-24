class Todo {
  id int
  todo string
  completed bool
  userId int
}

function GetTodo() -> Todo {
  baml.fetch_as<Todo>(baml.HttpRequest {
    method: baml.HttpMethod.Get,
    url: "https://dummyjson.com/todos/1",
  })
}

function GetTodoMissingTypeArg() -> Todo {
  baml.fetch_as(baml.HttpRequest {
    method: baml.HttpMethod.Get,
    url: "https://dummyjson.com/todos/1",
  })
}

// error: Error validating: Generic function baml.fetch_as must have a type argument. Try adding a type argument like this: baml.fetch_as<Type>
//   -->  expr/builtin.baml:16
//    | 
// 15 | function GetTodoMissingTypeArg() -> Todo {
// 16 |   baml.fetch_as(baml.HttpRequest {
// 17 |     method: baml.HttpMethod.Get,
// 18 |     url: "https://dummyjson.com/todos/1",
// 19 |   })
//    | 
// error: Error validating: Generic function baml.fetch_as must have a type argument. Try adding a type argument like this: baml.fetch_as<Type>
//   -->  expr/builtin.baml:16
//    | 
// 15 | function GetTodoMissingTypeArg() -> Todo {
// 16 |   baml.fetch_as(baml.HttpRequest {
// 17 |     method: baml.HttpMethod.Get,
// 18 |     url: "https://dummyjson.com/todos/1",
// 19 |   })
//    | 
