client<llm> DeterministicReplayClient {
  provider openai
  options {
    model "gpt-5.4-nano"
    api_key env.BAML_CSHARP_REPLAY_API_KEY
    base_url env.BAML_CSHARP_REPLAY_BASE_URL
  }
}

class StreamEnvelope {
  required string
  done_required string @stream.done
  defaulted string
  state string
  done string @stream.done
}

function Deterministic(text: string) -> string {
  client DeterministicReplayClient
  prompt `
    Return the deterministic replay text for: ${text}
  `
}

function Structured(text: string) -> StreamEnvelope {
  client DeterministicReplayClient
  prompt `
    Return the deterministic replay object for: ${text}

    ${ctx.output_format}
  `
}
