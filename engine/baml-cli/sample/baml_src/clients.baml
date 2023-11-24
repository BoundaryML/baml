// Clients are interfaces to communicate with AI models.
// They are allowed to be defined in any file (or even multiple), but for organization
// purposes, we are putting them all in this file.

// In order to use this, you'll need an env variable called OPENAI_API_KEY
// This can be set in a .env file in the root of the project, or in your shell.

client<llm> GPT4 {
  provider baml-openai-chat

  
  options {
    // Options are directly passed to the OpenAI client.
    // So you can add any of the params like:
    model gpt-4-1106-preview
    api_key env.OPENAI_API_KEY

    // By default, the temperature is 0
    temperature 0.1
  }
}

// We get that OPENAI GPT4 may be unreliable, so you can use fallbacks.

// This will try to use GPT4, and if it fails, it will use GPT35.
client<llm> Main {
  provider baml-fallback
  options {
    // First try GPT4 client, if it fails, try GPT35 client.
    strategy [
      GPT4,
      GPT35
      // If you had more clients, you could add them here.
      // Anthropic
    ]
  }
}


client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    // If you had your own environment variable, you could do this:
    // You could use any environment variable here
    api_key env.OPENAI_API_KEY
  }
}

client<llm> GPT35_YES_NO {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    api_key env.OPENAI_API_KEY
    logit_bias {
      // When using logit bias baml providers can automatically convert the string to a token using the appropriate tokenizer.
      " yes" 100
      " no" 100
    }
  }
}

client<llm> GPTInstruct {
  // Notice here we use the suffix "-completion"
  // This is because the instruct model is not a chat model.
  provider baml-openai-completion
  options {
    model gpt-3.5-instruct
    api_key env.OPENAI_API_KEY
  }
}


// This is a client for azure

// client<llm> AzureGPT4 {
//     // Instead of using baml-openai-chat, we use baml-azure-chat or baml-azure-completion.
//     provider baml-azure-chat
//     options {
//         api_key env.AZURE_API_KEY
//         api_base env.AZURE_API_BASE
//         api_version env.AZURE_API_VERSION
//         api_type env.AZURE_API_TYPE
//         engine gpt-4 // From azure
//         max_tokens 400
//         request_timeout 60
//     }
// }


// This is a client for anthropic

// client<llm> Anthropic {
//   // Anthropic doesn't have a chat model, so we use the completion provider.
//   provider baml-anthropic
//   options {
//     model claude
//   }
// }


// We don't yet support Google Vertex, or Llama, with baml native providers.
// You can write your own provider, by implementing the provider interface, and
// refer to that interface in the provider field.

// See examples here:
// baml-anthropic: https://github.com/GlooHQ/baml/blob/canary/clients/python/baml_core/registrations/providers/anthropic_provider.py
// baml-openai-chat: https://github.com/GlooHQ/baml/blob/canary/clients/python/baml_core/registrations/providers/openai_completion_provider.py
// baml-fallback: https://github.com/GlooHQ/baml/blob/canary/clients/python/baml_core/registrations/providers/fallback_provider.py
