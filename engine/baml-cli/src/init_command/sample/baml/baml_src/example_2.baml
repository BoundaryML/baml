enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

class Message {
  role string
  message string
}


function ClassifyConversation(messages: Message[]) -> Category[] {
  client $GENERATED_BAML_CLIENT$
  prompt #"
    Classify the following conversation into following:
    {{ ctx.output_schema }}


    {% for m in messages %}
    {{ _.chat(m.role) }}
    {{ m.message }}
    {% endfor %}

  "#
}
