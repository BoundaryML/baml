retry_policy DefaultRetryPolicy {
  max_retries 2
  strategy {
    type constant_delay
    delay_ms 1
  }
  options {
  }
}
 
retry_policy DelayRetryPolicy {
  max_retries 2
  strategy {
    type exponential_backoff
    delay_ms 1
  }
  options {
  }
}

client<llm> ResilientGPT4 {
  provider baml-fallback
  options {
    strategy [
      {
        client AZURE_DEFAULT
      },
      {
        client AZURE_GPT4
      },
      {
        client LARGE_RESPONSE
      }
    ]
  }
}

// test
client<llm> AZURE_DEFAULT {
    provider baml-openai-chat
    // retry_policy DefaultRetryPolicy
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> AZURE_GPT4 {
    provider baml-openai-chat
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> AZURE_YES_NO {
    provider baml-openai-chat
    retry_policy DefaultRetryPolicy
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> LARGE_RESPONSE {
    provider baml-openai-chat
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> Anthropic {
    provider baml-anthropic-chat
    options {
      api_key env.ANTHROPIC_API_KEY
      model claude-3-opus-20240229
      max_tokens_to_sample 300
    }
}