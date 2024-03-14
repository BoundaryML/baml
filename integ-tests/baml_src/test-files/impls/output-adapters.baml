// Add a function with an output adapter.
function FnTestOutputAdapter {
  input string
  output string
}

class ModifiedOutput {
  // alias just to test further
  reasoning string @alias(REASONING)
  answer string @alias(ANSWER)
}

impl<llm, FnTestOutputAdapter> v1 {
  client GPT35

  adapter<ModifiedOutput, output> python#"
    return arg.answer
  "#
  prompt #"
    Question: What is the capital of France?

    Return in this format:
    {#print_type(output)}

    JSON:
  "#
}