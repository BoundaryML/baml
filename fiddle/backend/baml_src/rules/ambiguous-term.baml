function AmbiguousTerm {
  input string
  output LinterOutput[]
}

impl<llm, AmbiguousTerm> version1 {
  client GPT4Turbo
  prompt #"
    You are a powerful AI linter that catches ambiguous terms and phrases in users' INSTRUCTIONS.

    A term is ambiguous when it is mentioned in the INSTRUCTIONS but there is no previous reference to it. If a synonym is used, you should suggest to use the same word as the previous reference.

    You can also catch terms that are nonsensical or out of context. Ignore anything in {hashtag...} tags.

    When you run analyze, ignore the properties on these objects like "client GPT4".

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
