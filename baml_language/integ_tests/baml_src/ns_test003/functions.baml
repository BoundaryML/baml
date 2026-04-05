client<llm> GPT4oMiniLocal {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

function AnalyzeSentiment(text: string) -> Sentiment {
  client GPT4oMiniLocal
  prompt #"
    Analyze the sentiment of the following text.

    Respond in JSON with this exact schema:
    {"label": "positive" or "negative" or "neutral", "confidence": 0.0 to 1.0}

    Text:
    {{ text }}

    JSON:
  "#
}
