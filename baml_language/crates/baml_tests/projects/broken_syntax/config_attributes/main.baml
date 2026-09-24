// Attributes belong to class, enum, interface, and function declarations.
// A config block has none, so the parser rejects them instead of dropping
// them on the floor.

client<llm> Annotated {
  @@description("not a declaration")
  provider openai
  options {
    model "gpt-4o" @alias("m")
    @@stream.done
  }
  retry_policy Retry @description("x")
}

retry_policy Retry {
  max_retries 3 @alias("n")
}
