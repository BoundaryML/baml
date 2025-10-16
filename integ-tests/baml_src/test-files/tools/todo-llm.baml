class AddTodoItem {
  type "add_todo_item" @stream.not_null
  item string
  time string
  description string @description("20 word description of the item")
  @@stream.done
}

class TodoMessageToUser {
  type "todo_message_to_user" @stream.not_null
  message string @description("A message to the user, about 50 words long")
}

type TodoTool = AddTodoItem | TodoMessageToUser

function ChooseTodoTools(query: string) -> TodoTool[] {
  client GPT4
  prompt #"
    Choose tools to satisfy the user query.
    For example, if they ask for "5 todo items for learning chess",
    return a list of 5 "add_todo_item" objects and single "todo_message_to_user"
    object. All requests should end with a "todo_message_to_user" object.

    {{ ctx.output_format }}
  "#
}

test ChooseTodoTools {
  functions [ChooseTodoTools]
  args {
    query "5 todo items for learning chess"
  }
}
