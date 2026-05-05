// Reads `OPENAI_API_KEY` from the process env. The
// `__build_request` companion only constructs the request body and
// never sends it, but `ExtractResume(...)` itself does — run via
// `infisical run -- ./run.sh` to inject the key.
client<llm> StubClient {
  provider openai
  options {
    model "gpt-4o"
    api_key env.OPENAI_API_KEY
  }
}

// LLM function. The compiler auto-synthesizes the four companions
// (`$build_request`, `$render_prompt`, `$parse`, `$parse_stream`); the
// demo exercises `$build_request` to prove the modular API path runs
// end-to-end without an LLM round-trip.
function ExtractResume(texts: string[]) -> Resume[] {
  client StubClient
  prompt #"
    Extract a resume from each of the following texts. Return one
    Resume per input text, in the same order as the inputs.

    Respond in JSON as an array where every element has this exact
    schema:
    {
      "name": "string",
      "email": "string or null",
      "addresses": [{"street": "string", "city": "string", "zip": "string or null"}],
      "scores": { "<subject>": "int" },
      "sentiment": "POSITIVE | NEGATIVE | NEUTRAL",
      "contact": { "country_code": "int", "digits": "string" }
    }

    Texts:
    {% for t in texts %}
    {{ loop.index }}. {{ t }}
    {% endfor %}

    JSON array:
  "#
}

// Drive the auto-synthesized `ExtractResume$stream` companion entirely
// inside BAML — the Python bridge can't currently round-trip a live
// `baml.llm.Stream`, so we count the partial yields here and return
// the count alongside the final value. Exposed as a static method on
// `StreamedExtractResult` because `client_codegen.rs:326` filters
// non-LLM free functions out of the codegen pool.
class StreamedExtractResult {
  partial_count int
  final Resume[]

  function run(texts: string[]) -> StreamedExtractResult {
    let stream = ExtractResume$stream(texts)
    let count: int = 0
    let i: int = 0
    while (i < 100) {
      let n = stream.next()
      log.info({ event: "stream.next", index: i, value: n })
      count = count + 1
      i = i + 1
    }
    let final_value: Resume[] = stream.final()
    StreamedExtractResult { partial_count: count, final: final_value }
  }
}

// Lower-level probe: bypass `Stream`/`StreamCache`/`StreamAccumulator`
// entirely and just hit `baml.http.fetch_sse(...)` on the request that
// `$build_request_stream` produced. If the SSE plumbing works, we
// should see N>0 chunks; if `Stream.next()` was bailing out for
// reasons unrelated to fetch_sse, this will still see chunks.
class FetchSseResult {
  count int
  first_chunk string
  last_chunk string

  function run(texts: string[]) -> FetchSseResult {
    let req = ExtractResume$build_request_stream(texts)
    let sse = baml.http.fetch_sse(req)
    let count: int = 0
    let first: string = ""
    let last: string = ""
    while (true) {
      let n = sse.next()
      match (n) {
        _: null => { break; }
        let chunk: string => {
          if (count == 0) {
            first = chunk
          }
          last = chunk
          count = count + 1
        }
      }
    }
    FetchSseResult { count: count, first_chunk: first, last_chunk: last }
  }
}
