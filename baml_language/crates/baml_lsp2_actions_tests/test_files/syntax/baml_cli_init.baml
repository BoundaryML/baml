// Learn more about clients at https://docs.boundaryml.com/docs/snippets/clients/overview

client CustomGPT5 = openai.ResponsesClient.new(
  model = "gpt-5",
  api_key = env.OPENAI_API_KEY,
);

// Retries compose by wrapping the inner client value.
client CustomGPT5Mini = ai.clients.Retry.new(
  inner = openai.ResponsesClient.new(model = "gpt-5-mini", api_key = env.OPENAI_API_KEY),
  max_attempts = 2,
);

// Openai with chat completion
client CustomGPT5Chat = openai.ResponsesClient.new(
  model = "gpt-5",
  api_key = env.OPENAI_API_KEY,
);

// Latest Anthropic Claude 4 models
client CustomOpus4 = anthropic.AnthropicClient.new(
  model = "claude-opus-4-1-20250805",
  api_key = env.ANTHROPIC_API_KEY,
);

client CustomSonnet4 = anthropic.AnthropicClient.new(
  model = "claude-sonnet-4-20250514",
  api_key = env.ANTHROPIC_API_KEY,
);

client CustomHaiku = ai.clients.Retry.new(
  inner = anthropic.AnthropicClient.new(
    model = "claude-haiku-4-5",
    api_key = env.ANTHROPIC_API_KEY,
  ),
  max_attempts = 3,
);

// Example Google AI client (uncomment to use)
// client CustomGemini = google.GoogleClient.new(
//   model = "gemini-2.5-pro",
//   api_key = env.GOOGLE_API_KEY,
// );

// https://docs.boundaryml.com/docs/snippets/clients/round-robin
// This will alternate between the two clients
client CustomFast = ai.clients.RoundRobin.new([CustomGPT5Mini, CustomHaiku]);

// https://docs.boundaryml.com/docs/snippets/clients/fallback
// This will try the clients in order until one succeeds
client OpenaiFallback = ai.clients.Fallback { members: [CustomGPT5Mini, CustomGPT5] };

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
  client: "openai/gpt-5-mini" // Set OPENAI_API_KEY to use this client.
  prompt: `
    Extract from this content:
    ${resume}

    ${ctx.output_format}
  `
}



// Test the function with a sample resume. Open the VSCode playground to run this.
test vaibhav_resume {
  functions [ExtractResume]
  args {
    resume "
      Vaibhav Gupta
      vbv@boundaryml.com

      Experience:
      - Founder at BoundaryML
      - CV Engineer at Google
      - CV Engineer at Microsoft

      Skills:
      - Rust
      - C++
    "
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
