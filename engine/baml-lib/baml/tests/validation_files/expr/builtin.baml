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
