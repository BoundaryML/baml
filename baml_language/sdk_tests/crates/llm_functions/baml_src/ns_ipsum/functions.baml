client<llm> Stub {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

// `classify_sentiment() -> Sentiment` from 09a.
function ClassifySentiment(text: string) -> Sentiment {
  client Stub
  prompt #"
    Classify the sentiment of the following text.

    Respond in JSON with one of: POSITIVE, NEGATIVE, NEUTRAL.

    Text:
    {{ text }}

    JSON:
  "#
}
