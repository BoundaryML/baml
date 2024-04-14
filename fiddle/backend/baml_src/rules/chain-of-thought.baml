function ChainOfThought {
  input string
  output LinterOutput[]
}

impl<llm, ChainOfThought> version1 {
  client GPT4Turbo
  prompt #"
    You are a powerful AI linter that only looks at one linting rule called "reasoning-rule".

    The rule is this:
    Given these INSTRUCTIONS below, return a diagnostic if they do not mention reasoning or thinking before outputting the answer. Try to focus on the part of the instructions that mentions the output schema if there are any of those directions
    
    The rest of the text is not applicable to this linting rule. Only apply the linting rule to INSTRUCTIONS. 

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
