// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview

// Using the new OpenAI Responses API for enhanced formatting
client<llm> CustomGPT5 {
  provider openai-responses
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

client<llm> CustomGPT5Mini {
  provider openai-responses
  retry_policy Exponential
  options {
    model "gpt-5-mini"
    api_key env.OPENAI_API_KEY
  }
}

// Openai with chat completion
client<llm> CustomGPT5Chat {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

// Latest Anthropic Claude 4 models
client<llm> CustomOpus4 {
  provider anthropic
  options {
    model "claude-opus-4-1-20250805"
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> CustomSonnet4 {
  provider anthropic
  options {
    model "claude-sonnet-4-20250514"
    api_key env.ANTHROPIC_API_KEY
  }
}

client<llm> CustomHaiku {
  provider anthropic
  retry_policy Constant
  options {
    model "claude-3-5-haiku-20241022"
    api_key env.ANTHROPIC_API_KEY
  }
}

// Example Google AI client (uncomment to use)
// client<llm> CustomGemini {
//   provider google-ai
//   options {
//     model "gemini-2.5-pro"
//     api_key env.GOOGLE_API_KEY
//   }
// }

// Example AWS Bedrock client (uncomment to use)
// client<llm> CustomBedrock {
//   provider aws-bedrock
//   options {
//     model "anthropic.claude-sonnet-4-20250514-v1:0"
//     region "us-east-1"
//     // AWS credentials are auto-detected from env vars
//   }
// }

// Example Azure OpenAI client (uncomment to use)
// client<llm> CustomAzure {
//   provider azure-openai
//   options {
//     model "gpt-5"
//     api_key env.AZURE_OPENAI_API_KEY
//     base_url "https://MY_RESOURCE_NAME.openai.azure.com/openai/deployments/MY_DEPLOYMENT_ID"
//     api_version "2024-10-01-preview"
//   }
// }

// Example Vertex AI client (uncomment to use)
// client<llm> CustomVertex {
//   provider vertex-ai
//   options {
//     model "gemini-2.5-pro"
//     location "us-central1"
//     // Uses Google Cloud Application Default Credentials
//   }
// }

// Example Ollama client for local models (uncomment to use)
// client<llm> CustomOllama {
//   provider openai-generic
//   options {
//     base_url "http://localhost:11434/v1"
//     model "llama4"
//     default_role "user" // Most local models prefer the user role
//     // No API key needed for local Ollama
//   }
// }

// https://docs.boundaryml.com/docs/snippets/clients/round-robin
client<llm> CustomFast {
  provider round-robin
  options {
    // This will alternate between the two clients
    strategy [CustomGPT5Mini, CustomHaiku]
  }
}

// https://docs.boundaryml.com/docs/snippets/clients/fallback
client<llm> OpenaiFallback {
  provider fallback
  options {
    // This will try the clients in order until one succeeds
    strategy [CustomGPT5Mini, CustomGPT5]
  }
}

// https://docs.boundaryml.com/docs/snippets/clients/retry
retry_policy Constant {
  max_retries 3
  strategy {
    type constant_delay
    delay_ms 200
  }
}

retry_policy Exponential {
  max_retries 2
  strategy {
    type exponential_backoff
    delay_ms 300
    multiplier 1.5
    max_delay_ms 10000
  }
}
// Defining a data model.
class Resume {
  name string
  email string
  experience string[]
  skills string[]
}

// Create a function to extract the resume from a string.
function ExtractResume(resume: string) -> Resume {
  // Specify a client as provider/model-name
  // You can also use custom LLM params with a custom client name from clients.baml like "client CustomGPT5" or "client CustomSonnet4"
  client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
  prompt #"
    Extract from this content:
    {{ resume }}

    {{ ctx.output_format }}
  "#
}



// Test the function with a sample resume. Open the VSCode playground to run this.
test vaibhav_resume {
  functions [ExtractResume]
  args {
    resume #"
      Vaibhav Gupta
      vbv@boundaryml.com

      Experience:
      - Founder at BoundaryML
      - CV Engineer at Google
      - CV Engineer at Microsoft

      Skills:
      - Rust
      - C++
    "#
  }
}

//----
//- diagnostics
// Error: Expected RBrace, found RetryPolicy
//     ╭─[ 0:14:3 ]
//     │
//  14 │   retry_policy Exponential
//     │   ──────┬─────  
//     │         ╰─────── Expected RBrace, found RetryPolicy
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected retry policy body, found Word
//     ╭─[ 0:15:3 ]
//     │
//  15 │   options {
//     │   ───┬───  
//     │      ╰───── Expected retry policy body, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:15:3 ]
//     │
//  15 │   options {
//     │   ───┬───  
//     │      ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:15:11 ]
//     │
//  15 │   options {
//     │           ┬  
//     │           ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:16:5 ]
//     │
//  16 │     model "gpt-5-mini"
//     │     ──┬──  
//     │       ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:16:11 ]
//     │
//  16 │     model "gpt-5-mini"
//     │           ┬  
//     │           ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:16:12 ]
//     │
//  16 │     model "gpt-5-mini"
//     │            ─────┬────  
//     │                 ╰────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:16:22 ]
//     │
//  16 │     model "gpt-5-mini"
//     │                      ┬  
//     │                      ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:5 ]
//     │
//  17 │     api_key env.OPENAI_API_KEY
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Env
//     ╭─[ 0:17:13 ]
//     │
//  17 │     api_key env.OPENAI_API_KEY
//     │             ─┬─  
//     │              ╰─── Expected top-level declaration, found Env
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:17:16 ]
//     │
//  17 │     api_key env.OPENAI_API_KEY
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:17 ]
//     │
//  17 │     api_key env.OPENAI_API_KEY
//     │                 ───────┬──────  
//     │                        ╰──────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:18:3 ]
//     │
//  18 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:19:1 ]
//     │
//  19 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found RetryPolicy
//     ╭─[ 0:49:3 ]
//     │
//  49 │   retry_policy Constant
//     │   ──────┬─────  
//     │         ╰─────── Expected RBrace, found RetryPolicy
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected retry policy body, found Word
//     ╭─[ 0:50:3 ]
//     │
//  50 │   options {
//     │   ───┬───  
//     │      ╰───── Expected retry policy body, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:50:3 ]
//     │
//  50 │   options {
//     │   ───┬───  
//     │      ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:50:11 ]
//     │
//  50 │   options {
//     │           ┬  
//     │           ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:51:5 ]
//     │
//  51 │     model "claude-3-5-haiku-20241022"
//     │     ──┬──  
//     │       ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:51:11 ]
//     │
//  51 │     model "claude-3-5-haiku-20241022"
//     │           ┬  
//     │           ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:51:12 ]
//     │
//  51 │     model "claude-3-5-haiku-20241022"
//     │            ────────────┬────────────  
//     │                        ╰────────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:51:37 ]
//     │
//  51 │     model "claude-3-5-haiku-20241022"
//     │                                     ┬  
//     │                                     ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:52:5 ]
//     │
//  52 │     api_key env.ANTHROPIC_API_KEY
//     │     ───┬───  
//     │        ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Env
//     ╭─[ 0:52:13 ]
//     │
//  52 │     api_key env.ANTHROPIC_API_KEY
//     │             ─┬─  
//     │              ╰─── Expected top-level declaration, found Env
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Dot
//     ╭─[ 0:52:16 ]
//     │
//  52 │     api_key env.ANTHROPIC_API_KEY
//     │                ┬  
//     │                ╰── Expected top-level declaration, found Dot
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:52:17 ]
//     │
//  52 │     api_key env.ANTHROPIC_API_KEY
//     │                 ────────┬────────  
//     │                         ╰────────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:53:3 ]
//     │
//  53 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:54:1 ]
//     │
//  54 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Comma
//      ╭─[ 0:112:29 ]
//      │
//  112 │     strategy [CustomGPT5Mini, CustomHaiku]
//      │                             ┬  
//      │                             ╰── Expected config key, found Comma
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected config key, found Comma
//      ╭─[ 0:121:29 ]
//      │
//  121 │     strategy [CustomGPT5Mini, CustomGPT5]
//      │                             ┬  
//      │                             ╰── Expected config key, found Comma
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected client name, found Quote
//      ╭─[ 0:155:10 ]
//      │
//  155 │   client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
//      │          ┬  
//      │          ╰── Expected client name, found Quote
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//      ╭─[ 0:155:10 ]
//      │
//  155 │   client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
//      │          ┬  
//      │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai-responses', found Word
//      ╭─[ 0:155:11 ]
//      │
//  155 │   client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
//      │           ────────┬───────  
//      │                   ╰───────── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai-responses', found Word
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//      ╭─[ 0:155:27 ]
//      │
//  155 │   client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
//      │                           ┬  
//      │                           ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-5-mini', found Word
//      ╭─[ 0:155:28 ]
//      │
//  155 │   client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
//      │                            ─────┬────  
//      │                                 ╰────── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-5-mini', found Word
//      │ 
//      │ Note: Error code: E0010
// ─────╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//      ╭─[ 0:155:38 ]
//      │
//  155 │   client "openai-responses/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
//      │                                      ┬  
//      │                                      ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//      │ 
//      │ Note: Error code: E0010
// ─────╯
