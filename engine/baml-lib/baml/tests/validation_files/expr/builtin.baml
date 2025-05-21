class Todo {
  id int
  todo string
  completed bool
  userId int 
}

fn GetTodo() -> Todo {
  std::fetch_value<Todo>(std::Request {
    base_url: "https://dummyjson.com/todos/1",
    headers: {},
    query_params: {},
  })
}
