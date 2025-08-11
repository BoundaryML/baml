client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant", "system"]
    remap_roles {
      user "human"
      assistant "ai"
      system "instructions"
    }
    default_role "user"
  }
}

function TestRemap(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("user") }}
    Test message: {{ input }}
  "#
}