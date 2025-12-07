// Client configuration for GEPA reflection model
//
// This file configures the LLM used for prompt optimization reflection.
// You can customize this to use a different model or provider.
//
// Run `baml-cli optimize --reset-gepa-prompts` to restore defaults.

client<llm> ReflectionModel {
    provider openai
    options {
        model "gpt-4o"
        temperature 0.7
        max_tokens 8000
    }
}

// Alternative configurations (uncomment to use):

// Anthropic Claude
// client<llm> ReflectionModel {
//     provider anthropic
//     options {
//         model "claude-sonnet-4-20250514"
//         temperature 0.7
//         max_tokens 8000
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
