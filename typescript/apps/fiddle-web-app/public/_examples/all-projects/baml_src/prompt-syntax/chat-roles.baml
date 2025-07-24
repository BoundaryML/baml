// This will be available as an enum in your Python and Typescript code.
enum MyCategory {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

function ClassifyMessageWithRoles(input: string) -> MyCategory {
  client GPT4o

  prompt #"
    {# _.role("system") starts a system message #}
    {{ _.role("system") }}

    Classify the following INPUT into ONE
    of the following categories:

    {{ ctx.output_format }}

    {# This starts a user message #}
    {{ _.role("user") }}

    INPUT: {{ input }}

    Response:
  "#
}

test Test1 {
  functions [ClassifyMessageWithRoles]
  args {
    input "Can't access my account using my usual login credentials, and each attempt results in an error message stating 'Invalid username or password.' I have tried resetting my password using the 'Forgot Password' link, but I haven't received the promised password reset email."
  }
}