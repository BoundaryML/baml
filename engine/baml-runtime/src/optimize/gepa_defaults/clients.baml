// Client configuration for GEPA reflection model
//
// This file configures the LLM used for prompt optimization reflection.
// You can customize this to use a different model or provider.
//
// Run `baml-cli optimize --reset-gepa-prompts` to restore defaults.

// Anthropic Claude
client<llm> ReflectionModel {
    provider anthropic
    options {
        temperature 0.7
        model "claude-opus-4-5-20251101"
    }
}

// Alternative configurations (uncomment to use):

// client<llm> ReflectionModel {
//     provider openai
//     options {
//         model "gpt-4o"
//     }
// }

// Azure OpenAI
// client<llm> ReflectionModel {
//     provider azure-openai
//     options {
//         resource_name "your-resource"
//         deployment_id "your-deployment"
//         api_version "2024-02-15-preview"
//         temperature 0.7
//         max_tokens 8000
//     }
// }
