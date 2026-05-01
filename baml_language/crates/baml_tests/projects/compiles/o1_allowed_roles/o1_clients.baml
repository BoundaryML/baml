// Verify that o1 models default to ["user", "assistant"] (no system role),
// while non-o1 models keep the standard ["system", "user", "assistant"].

client<llm> O1Client {
  provider openai
  options {
    model "o1"
    api_key "sk-test"
  }
}

client<llm> O1MiniClient {
  provider openai
  options {
    model "o1-mini"
    api_key "sk-test"
  }
}

client<llm> Gpt4oClient {
  provider openai
  options {
    model "gpt-4o"
    api_key "sk-test"
  }
}

// o1 with explicit allowed_roles should respect the override
client<llm> O1WithExplicitRoles {
  provider openai
  options {
    model "o1"
    api_key "sk-test"
    allowed_roles ["system", "user", "assistant"]
  }
}
