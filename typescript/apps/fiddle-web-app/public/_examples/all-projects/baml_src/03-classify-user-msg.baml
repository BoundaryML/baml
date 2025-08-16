// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

class Message {
  userName string
  message string
}

// inputs can be more complex than just a string
function ClassifyMessage(message: Message) -> Category {
  client GPT4o

  prompt #"
    Classify the following INPUT into ONE
    of categories listed.

    INPUT:
    ---
    {{ message.userName }}: {{ message.message }}
    ---

    {{ ctx.output_format }}

    Response:
  "#
}

test Test1 {
  functions [ClassifyMessage]
  args {
    message {
      userName "Alice"
      message "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
    }
  }
}