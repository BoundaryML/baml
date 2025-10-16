class MyUserMessage {
  user_name string
  content string
}

function FunctionWithLoops(messages: MyUserMessage[]) -> string {
  client GPT4o
  prompt #"
    {% for message in messages %}
      {{ message.user_name }}: {{ message.content }}
    {% endfor %}
  "#
}

test TestName {
  functions [FunctionWithLoops]
  args {
    messages [
      {
        user_name "Alice"
        content "Hello!"
      }
      {
        user_name "Bob"
        content "Hi!"
      }
    ]
  }
}
