class Todo {
  id int
  todo string
  completed bool
  userId int
}

function GetTodo() -> Todo {
  std::fetch_value<Todo>(std::Request {
    method: Get,
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}
