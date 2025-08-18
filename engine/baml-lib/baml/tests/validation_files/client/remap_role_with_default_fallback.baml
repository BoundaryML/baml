client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "system"]
    default_role "system"
    remap_roles {
      system "instructions"
    }
  }
}

function TestRemapWithFallback(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("assistant") }}
    This should fallback to system role and then be remapped to instructions: {{ input }}
  "#
}