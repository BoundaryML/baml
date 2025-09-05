client<llm> DeepSeekAzure {
    provider openai-generic
    options {
        base_url "https://DeepSeek-R1-dtjbj.eastus2.models.ai.azure.com"
        api_key env.DEEPSEEK_AZURE_API_KEY
        max_tokens 10
    }
}

function TellStory(story: string) -> string {
  client "openai/gpt-4o-mini"
  prompt #"
    You are a storyteller. Tell a story about the following:
    {{ _.role("user") }} {{ story }}
  "#
}

test TellStory {
  functions [TellStory]
  args {
    story #"
      Once upon a time, there was a cat who loved to play with yarn.
    "#
  }
}
