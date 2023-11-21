// Clients are interfaces to communicate with AI models.
// They are allowed to be defined in any file (or even multiple), but for organization
// purposes, we are putting them all in this file.

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4-1106-preview
  }
}

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
  }
}

client<llm> GPTInstruct {
  provider baml-openai-completion
  options {
    model gpt-3.5-instruct
  }
}

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
  }
}
