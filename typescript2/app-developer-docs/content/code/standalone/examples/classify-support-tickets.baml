// docs:start example
enum Category {
  Refund
  CancelOrder
  TechnicalSupport
  AccountIssue
  Question
}

function classify_message(input: string) -> Category {
  client: openai.ResponsesClient.new(model = "gpt-4.1-mini", api_key = env.OPENAI_API_KEY)
  prompt: `
    ${role("system")}
    Classify the support message into one category.
    ${ctx.output_format()}

    ${role("user")}
    ${input}
  `
}
// docs:end example
