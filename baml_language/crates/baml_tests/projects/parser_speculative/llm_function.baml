function ExtractData(text string) -> JsonData {
  client GPT4
  prompt #"
    Extract structured data from: {{ text }}
    Return as JSON.
  "#
}
