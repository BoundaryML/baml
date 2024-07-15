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
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
} 


client<llm> GPT4o {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
} 


client<llm> GPT4Turbo {
  retry_policy Bar
  provider openai
  options {
    model gpt-4-turbo
    api_key env.OPENAI_API_KEY
  }
} 

client<llm> GPT35 {
  provider openai
  options {
    model "gpt-3.5-turbo"
    api_key env.OPENAI_API_KEY
  }
}

client<llm> GPT35LegacyProvider {
  provider openai
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
  options {
    model gemini-1.5-pro-001
    api_key env.GOOGLE_API_KEY
    safetySettings {
      category HARM_CATEGORY_HATE_SPEECH
      threshold BLOCK_LOW_AND_ABOVE

    }
  }
}

client<llm> Vertex {
  provider vertex-ai
  
  options {
    model gemini-1.5-pro
    project_id anish-testing-426119
    location us-central1
    // base_url https://us-central1-aiplatform.googleapis.com/v1/projects/anish-testing-426119/locations/us-central1/publishers/google/models/
  }
}


client<llm> AwsBedrock {
  provider aws-bedrock
  options {
    inference_configuration {
      max_tokens 100
    }
    model_id "anthropic.claude-3-haiku-20240307-v1:0"
    // model_id "meta.llama3-8b-instruct-v1:0"
    // model_id "mistral.mistral-7b-instruct-v0:2"
    api_key ""
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
