// test
client<llm> AZURE_DEFAULT {
    provider openai-chat
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> AZURE_GPT4 {
    provider openai-chat
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> AZURE_YES_NO {
    provider openai-chat
    retry DefaultRetryPolicy
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

client<llm> LARGE_RESPONSE {
    provider openai-chat
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokens 400
    }
}

