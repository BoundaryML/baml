// Inline shorthand client — anthropic counterpart to ns_lorem's openai
// shorthand. The `from_shorthand` rust path must source api_key from
// ANTHROPIC_API_KEY for `anthropic` and OPENAI_API_KEY for `openai`;
// see `test_classify_sentiment_build_request_includes_anthropic_api_key`
// in customizable/test_main.py.
function ClassifySentiment(text: string) -> Sentiment {
  client "anthropic/claude-3-5-sonnet-20241022"
  prompt #"
    Classify the sentiment of the following text.

    Respond in JSON with one of: POSITIVE, NEGATIVE, NEUTRAL.

    Text:
    {{ text }}

    JSON:
  "#
}
