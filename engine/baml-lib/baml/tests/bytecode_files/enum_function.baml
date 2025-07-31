enum Sentiment {
  Positive
  Negative
  Neutral
}

function AnalyzeSentiment(text: string) -> Sentiment {
  client "openai/gpt-4o-mini"
  prompt #"
    Analyze the sentiment of this text: {{ text }}
    
    Return one of: Positive, Negative, or Neutral
  "#
}