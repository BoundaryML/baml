// Compiler-corpus coverage for generated `$stream` companions. This pins both
// primitive and namespaced class return types to the canonical
// `ai.stream.Stream<TPartial, TFinal>` surface. Runnable provider/SSE and host
// bridge coverage lives in the SDK replay tests; provider-wire prompt coverage
// lives in `prompt_tag_e2e`.

// Never contacted: the URL points at a dead localhost port. The declaration
// only supplies provider/model/key metadata for prompt rendering.
client StreamParseClient = openai.ResponsesClient.new(
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
