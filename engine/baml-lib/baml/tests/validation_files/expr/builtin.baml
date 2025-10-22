class Todo {
  id int
  todo string
  completed bool
  userId int
}

function GetTodo() -> Todo {
  baml.fetch_value<Todo>(baml.Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}

function GetTodoMissingTypeArg() -> Todo {
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
//    | 
