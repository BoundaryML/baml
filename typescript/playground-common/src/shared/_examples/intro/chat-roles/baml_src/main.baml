// This will be available as an enum in your Python and Typescript code.
enum Category {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessage(input: string) -> Category {
  client GPT4Turbo

  prompt #"
    {# _.chat("system") starts a system message #}
    {{ _.chat("system") }}

    Classify the following INPUT into ONE
    of the following categories:

    {{ ctx.output_format }}

    {# This starts a user message #}
    {{ _.chat("user") }}

    INPUT: {{ input }}

    Response:
  "#
}

test Test1 {
  functions [ClassifyMessage]
  args {
    input "Can't access my account using my usual login credentials, and each attempt results in an error message stating 'Invalid username or password.' I have tried resetting my password using the 'Forgot Password' link, but I haven't received the promised password reset email."
  }
}