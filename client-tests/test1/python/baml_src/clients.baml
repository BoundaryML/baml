// test
client<llm> AZURE_DEFAULT {
    provider openai
    {// fallback AZURE_YES_NO //}
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokes 400
    }
}

client<llm> AZURE_GPT4 {
    provider openai
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokes 400
    }
}

client<llm> AZURE_YES_NO {
    provider openai
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokes 400
    }
}

client<llm> LARGE_RESPONSE {
    provider openai
    options {
      model gpt-3.5-turbo
      api_key env.OPENAI_API_KEY
      request_timeout 45
      max_tokes 400
    }
}

