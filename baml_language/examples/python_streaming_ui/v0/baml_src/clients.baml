// v0 versions of the six scenario clients, copied from the user's config (and the #5041 stand-ins).
// Dropped on purpose: `http` timeouts (6-10s request limits would cut off this ~1 minute extraction)
// and retry policies. Vertex auth uses the test service account instead of gcloud impersonation.

// 1. AWS Bedrock
client<llm> BedrockHaiku45 {
  provider aws-bedrock
  options {
    model "global.anthropic.claude-haiku-4-5-20251001-v1:0"
    access_key_id env.BEDROCK_AWS_ACCESS_KEY_ID
    secret_access_key env.BEDROCK_AWS_SECRET_ACCESS_KEY
    region env.BEDROCK_AWS_REGION
    inference_configuration {
      max_tokens 64000
      temperature 0.0
    }
  }
}

// 2. Bedrock Mantle. gpt-oss is served under `/v1`, not `/openai/v1`.
client<llm> BedrockGptOss20b {
  provider openai-responses
  options {
    model "openai.gpt-oss-20b"
    api_key env.BEDROCK_MANTLE_API_KEY
    base_url "https://bedrock-mantle.us-east-1.api.aws/v1"
    reasoning {
      effort "low"
    }
    store false
  }
}

// 3. Azure AI Foundry (gpt-5-mini stand-in for AzureGpt56LunaLow).
client<llm> AzureGpt5MiniLow {
  provider openai-responses
  options {
    model "gpt-5-mini"
    api_key env.AZURE_OPENAI_API_KEY
    base_url env.AZURE_OPENAI_RESPONSES_BASE_URL
    headers {
      "api-key" env.AZURE_OPENAI_API_KEY
    }
    reasoning {
      effort "low"
    }
    store false
  }
}

// 4. Google AI Studio
client<llm> Gemini31FlashLiteMinimal {
  provider google-ai
  options {
    model "gemini-3.1-flash-lite"
    api_key env.GOOGLE_API_KEY
    generationConfig {
      thinkingConfig {
        thinkingLevel "minimal"
      }
    }
  }
}

// 5. Vertex AI
client<llm> VertexGeminiFlashLiteLow {
  provider vertex-ai
  options {
    model "gemini-3.5-flash-lite"
    location "global"
    credentials env.GOOGLE_APPLICATION_CREDENTIALS_CONTENT
    generationConfig {
      temperature 0.0
      thinkingConfig {
        thinkingLevel "low"
      }
    }
  }
}

// 6. Vertex Model Garden. The endpoint needs max_tokens; the user's client omits it.
client<llm> VertexLlama4ScoutRouter {
  provider openai-generic
  options {
    base_url env.VERTEX_LLAMA_OPENAI_BASE_URL
    api_key env.VERTEX_ACCESS_TOKEN
    model "meta/llama-4-scout-17b-16e-instruct-maas"
    temperature 0.0
    max_tokens 8192
  }
}
