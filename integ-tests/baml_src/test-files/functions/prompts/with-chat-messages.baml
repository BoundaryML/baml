
function PromptTestOpenAIChat(input: string) -> string {
  client GPT35
  prompt #"
    {{ _.role("system") }}
    You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {{ input }}
    
    {{ _.role("user") }}
    Tell me a haiku about {{ input }}
  "#
}

function PromptTestOpenAIChatNoSystem(input: string) -> string {
  client GPT35
  prompt #"
    You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {{ input }}
    
    {{ _.role("user") }}
    Tell me a haiku about {{ input }}
  "#
}

function PromptTestClaudeChat(input: string) -> string {
  client Claude
  prompt #"
    {{ _.role("system") }}
    You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {{ input }}
    
    {{ _.role("user") }}
    Tell me a haiku about {{ input }}
  "#
}

function PromptTestClaudeChatNoSystem(input: string) -> string {
  client Claude
  prompt #"
    You are an assistant that always responds in a very excited way with emojis and also outputs this word 4 times after giving a response: {{ input }}
    
    {{ _.role("user") }}
    Tell me a haiku about {{ input }}
  "#
}

test PromptTestOpenAIChat {
  functions [PromptTestClaude, PromptTestOpenAI, PromptTestOpenAIChat, PromptTestOpenAIChatNoSystem, PromptTestClaudeChat, PromptTestClaudeChatNoSystem]
  args {
    input "cats"
  }
}

test TestClaude {
  functions [PromptTestClaudeChatNoSystem]
  args {
    input "lion"
  }
}