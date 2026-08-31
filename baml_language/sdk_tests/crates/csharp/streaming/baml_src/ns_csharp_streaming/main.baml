// A `client Name = ...` declaration is evaluated during `$init`, which cannot
// run io sysops — so `env.NAME` names the variables instead of reading them and
// the provider resolves them at request time. The harness parent process sets
// BAML_CSHARP_REPLAY_* before spawning the native child, so the child's
// requests land on the in-process replay server.
client DeterministicReplayClient = openai.ResponsesClient.new(
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
  client: DeterministicReplayClient
  prompt: `
    Return the deterministic replay text for: ${text}
  `
}

function Structured(text: string) -> StreamEnvelope {
  client: DeterministicReplayClient
  prompt: `
    Return the deterministic replay object for: ${text}

    ${ctx.output_format()}
  `
}

function InspectMedia(photo: image) -> string {
  client: DeterministicReplayClient
  prompt: `${role("user")}Describe this image: ${photo}`
}
