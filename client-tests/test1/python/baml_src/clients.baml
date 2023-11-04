// test
client<llm> AZURE_DEFAULT {
    provider baml-openai-chat
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
    retry DefaultRetryPolicy
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

