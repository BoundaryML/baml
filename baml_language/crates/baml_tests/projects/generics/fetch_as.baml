class Todo {
    id int
    title string
    completed bool
    userId int
}

function GetTodo() -> Todo {
    baml.fetch_as<Todo>(baml.HttpRequest {
        method: baml.HttpMethod.Get,
        url: "dummyjson.com/todos/1",
    })
}