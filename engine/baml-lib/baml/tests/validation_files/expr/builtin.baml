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
