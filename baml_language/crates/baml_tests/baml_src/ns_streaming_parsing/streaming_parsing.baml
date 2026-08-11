// Compiler-corpus coverage for generated `$stream` companions. This pins both
// primitive and namespaced class return types to the canonical
// `ai.stream.Stream<TPartial, TFinal>` surface. Runnable provider/SSE and host
// bridge coverage lives in `_planv2/baml_src/tests/streaming.baml` and the SDK
// replay tests.

// Never contacted: the URL points at a dead localhost port and the parked
// streaming tests swapped it for a live in-test server before fetching. The
// declaration only supplies provider/model/key metadata for prompt rendering.
client StreamParseClient = openai.OpenAiClient.new(
    model = "gpt-4o",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

class Doc {
    title string
    body string?
    word_count int
}

function ExtractDoc(input: string) -> Doc {
    client StreamParseClient
    prompt `Say hello to ${input}`
}

function SayHello(input: string) -> string {
    client StreamParseClient
    prompt `Say hello to ${input}`
}
