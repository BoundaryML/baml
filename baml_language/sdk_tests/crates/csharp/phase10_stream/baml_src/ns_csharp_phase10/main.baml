// TODO(stream-migration): this crate's C# harness (Program.cs) drives the
// `$stream` companions, which were removed with the legacy LLM path and
// return via the new ai-world streaming machinery. The fixture below is
// migrated to the new client/prompt syntax so it keeps compiling; the
// streaming assertions in Program.cs stay parked until `$stream` lands.

// base_url/api_key resolve from env at request time; the harness parent
// process sets BAML_CSHARP_REPLAY_* before spawning the native child.
client DeterministicReplayClient = openai.OpenAiClient.new(
  model = "gpt-5.4-nano",
  api_key = env.BAML_CSHARP_REPLAY_API_KEY,
  base_url = env.BAML_CSHARP_REPLAY_BASE_URL,
);

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
