// Inline shorthand client — anthropic counterpart to ns_lorem's openai
// shorthand. The provider constructor resolves api_key from
// ANTHROPIC_API_KEY at request time.
function ClassifySentiment(text: string) -> Sentiment {
  client: "anthropic/claude-3-5-sonnet-20241022"
  prompt: `
    Classify the sentiment of the following text.

    Respond in JSON with one of: POSITIVE, NEGATIVE, NEUTRAL.

    Text:
    ${text}

    JSON:
  `
}
