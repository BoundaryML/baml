function AmbiguousTerm {
  input string
  output LinterOutput[]
}

impl<llm, AmbiguousTerm> version1 {
  client GPT4Turbo
  prompt #"
    You are a powerful AI linter that catches terms or phrases that are nonsensical or out of context. Ignore anything in {hashtag...} tags.

    Allow advanced prompting techniques that improve the model's results like:
    - Providing examples
    - Chain of thought
    - Tipping

    --------------------
    <INSTRUCTIONS>
    {#input}
    </INSTRUCTIONS>
    --------------------


    Output the diagnostic in this JSON format (only include these fields, and no others):
    {#print_type(output)}

    Output JSON Array:
  "#
}
