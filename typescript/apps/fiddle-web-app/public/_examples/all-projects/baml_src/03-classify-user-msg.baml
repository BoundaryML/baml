// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund @alias("give my money back")
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
function ClassifyMessage(enum_arg: Category) -> Category {
  client GPT4o

  prompt #"
    Classify the following INPUT into ONE
    of categories listed.

    INPUT:
    ---
    {{ enum_arg }}
    {{ Category.Refund }}
    ---

    {{ ctx.output_format }}

    Response:
  "#
}

test Test1 {
  functions [ClassifyMessage]
  args {
    enum_arg Refund
  }
}