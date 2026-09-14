// docs:start example
enum Category {
  Refund
  CancelOrder
  TechnicalSupport
  AccountIssue
  Question
}

function ClassifyMessage(input: string) -> Category {
  client: "openai/gpt-4o-mini"
  prompt: `
    Classify the following message into one category:

    ${input}

    ${ctx.output_format()}
  `
}
// docs:end example
