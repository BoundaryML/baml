// Generated SDKs do not expose stdlib methods, so hosts read a rendered
// `ai.Prompt` through user functions like these.

function prompt_text(prompt: ai.Prompt) -> string {
  prompt.text()
}

function prompt_messages(prompt: ai.Prompt) -> ai.PromptMessage[] {
  prompt.messages()
}
