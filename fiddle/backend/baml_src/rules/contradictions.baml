
class LinterOutput {
  
  reason string @description(#"
    Explain why the linting error was raised.
  "#)
  exactPhrase string @description(#"
    The phrase that triggered the linter error. Write it EXACTLY as it appears in the PROMPT. If it's more than 10 words, just match the first 10 words.
  "#)
  recommendation string? @description(#"
    A human-readable string that explains how to fix the linting error.
  "#)
  recommendation_reason string? @description(#"
    Explain why the recommendation is the best course of action.
  "#)
  fix string? @alias("fixedPhrase") @description(#"
    The fix for the linting error. You MUST start at the same location as the original phrase.
  "#)
}

function Contradictions {
  input string
  output LinterOutput[]
}

impl<llm, Contradictions> version1 {
  client GPT4Turbo
  prompt #"
    You are a powerful AI linter that will analyze CONTRADICTIONS in the text in between the <INSTRUCTIONS> tags and output a set of diagnostics matching the output schema.

    When there is a contradiction just make an assumption as to what the actual intent is and put that assumption in the "recommendation". The "fix" should incorporate that assumption.

    <INSTRUCTIONS>
    {#input}
    </INSTRUCTIONS>

    Output JSON format (only include these fields, and no others). Explain your reasoning in 2-3 brief sentences before writing out the json:
    {#print_type(output)}

    Output JSON Array:
  "#
}
