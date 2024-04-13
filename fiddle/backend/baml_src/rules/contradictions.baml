
class LinterOutput {
  exactPhrase string @description(#"
    The phrase that the linter matched on. Write it EXACTLY as it appears in the PROMPT. If it's more than 10 words, just match the first 10 words.
  "#)
  reason string @description(#"
    Explain why the linting error was raised.
  "#)
  severity string @description(#"
    'Error' if it's a major issue, 'Warning' if it's a minor issue that may not cause actual problems.
  "#)
  recommendation string? @description(#"
    A human-readable string that explains how to fix the linting error.
  "#)
  recommendation_reason string? @description(#"
    Explain why the recommendation is the best course of action.
  "#)
  fix string? @alias("fixedPhrase") @description(#"
    The fix for the linting error. This is a string that can be applied to the source code to fix the linting error. Just output the new string that will replace the spanMatch.
  "#)
}

function Contradictions {
  input string
  output LinterOutput[]
}

impl<llm, Contradictions> version1 {
  client GPT4
  prompt #"
    You are a powerful AI linter.

    The linting rule is:
    Contradictions: Find any contradictory statements in the PROMPT and output a set of diagnostics matching the output schema.

    The PROMPT is a set of directions written by a user. The user may have made a mistake in the directions, and you need to find and correct them (if possible) according to the rule. Ensure the original intent of the PROMPT is preserved as much as possible.

    For this rule, just make an assumption as to what the actual intent is and put that assumption in the "recommendation". The "fix" should incorporate that assumption.

    <PROMPT>
    {#input}
    </PROMPT>

    Output JSON format (only include these fields, and no others):
    {#print_type(output)}

    Output JSON Array:
  "#
}
