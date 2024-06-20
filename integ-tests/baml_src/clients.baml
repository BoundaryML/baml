retry_policy Bar {
  max_retries 3
  strategy {
    type exponential_backoff
  }
}

retry_policy Foo {
  max_retries 3
  strategy {
    type constant_delay
    delay_ms 100
  }
}

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
} 


client<llm> GPT4o {
  provider baml-openai-chat
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
} 


client<llm> GPT4Turbo {
  retry_policy Bar
  provider baml-openai-chat
  options {
    model gpt-4-turbo
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model "gpt-3.5-turbo"
    api_key env.OPENAI_API_KEY
  }
}

client<llm> Ollama {
  provider ollama
  options {
    model llama2
  }
}

client<llm> GPT35Azure {
  provider azure-openai
  options {
    resource_name "west-us-azure-baml"
    deployment_id "gpt-35-turbo-default"
    // base_url "https://west-us-azure-baml.openai.azure.com/openai/deployments/gpt-35-turbo-default"
    api_version "2024-02-01"
    api_key env.AZURE_OPENAI_API_KEY
  }
}

client<llm> Gemini {
  provider google-ai
  options{
    model "gemini-1.5-pro-001"
    api_key env.GOOGLE_API_KEY
  }
}


client<llm> Claude {
  provider anthropic
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
    max_tokens 1000
  }
}

client<llm> Resilient_SimpleSyntax {
  retry_policy Foo
  provider baml-fallback
  options {
    strategy [
      GPT4Turbo
      GPT35
      Lottery_SimpleSyntax
    ]
  }
} 
 
client<llm> Lottery_SimpleSyntax {
  provider baml-round-robin
  options {
    start 0
    strategy [
      GPT35
      Claude
    ]
  }
}
