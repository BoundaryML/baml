function PromptTest {
  input string
  output string
  default_impl claude_chat
}

impl<llm, PromptTest> claude_chat {
  client Claude
  prompt #"
    Tell me a haiku about {#input}
  "#
} 

impl<llm, PromptTest> openai_chat {
  client GPT35
  prompt #"
    Tell me a haiku about {#input}
  "#
}

impl<llm, PromptTest> bird_chat {
  client Lottery_ComplexSyntax
  //client Lottery_SimpleSyntax
  prompt #"
    Tell me about your maker, then give me a haiku about {#input}
  "#
}
