function ChainOfThought {
  input string
  output LinterOutput[]
}

impl<llm, ChainOfThought> version1 {
  client GPT4
  prompt #"
    You are a powerful AI linter tasked with fixing prompts given to AI models.

    The linting rule is:
    ChainOfThoughtValidation - Ensure that the user's PROMPT asks for reasoning before outputting the answer, like "before outputting your answer, please make sure to think step by step" or "please output your reasoning before you give the answer" or the reasoning step can be in the answer itself, but should be before any other properties (if it is json). Since the user is prompting an LLM model, and they are autoregressive.

    The PROMPT is a set of directions written by a user. The user may have made a mistake in the directions, and you need to find and correct them (if possible) according to the rule. Ensure the original intent of the PROMPT is preserved as much as possible.

    <PROMPT>
    {#input}
    </PROMPT>

    Output JSON format (only include these fields, and no others):
    {#print_type(output)}

    Output JSON Array:
  "#
}
