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