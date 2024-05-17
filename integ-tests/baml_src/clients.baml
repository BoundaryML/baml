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

client<llm> Claude {
  provider baml-anthropic-chat
  options {
    model claude-3-haiku-20240307
    api_key env.ANTHROPIC_API_KEY
    max_tokens 1000
  }
}

client<llm> Ollama {
  provider baml-ollama-chat
  options {
    model llama2
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

client<llm> Resilient_ComplexSyntax {
  provider baml-fallback
  options {
    strategy [
      {
        client GPT4Turbo
      }
      {
        client GPT35
      }
      {
        client Claude
      }
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

client<llm> Lottery_ComplexSyntax {
  provider baml-round-robin
  options {
    start 0
    strategy [
      {
        client GPT35
      }
      {
        client Claude
      }
    ]
  }
}