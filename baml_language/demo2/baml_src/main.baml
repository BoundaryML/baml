// Demo: google-ai provider routed through Vertex AI via env vars.
//
// Run with:
//   GOOGLE_GENAI_USE_VERTEXAI=1 \
//   GOOGLE_CLOUD_PROJECT=<project> \
//   GOOGLE_CLOUD_LOCATION=<location> \
//   baml run --from demo2 main
//
// No api_key: with GOOGLE_GENAI_USE_VERTEXAI=1 the request routes to the
// Vertex AI backend and authenticates with application-default credentials
// (`gcloud auth application-default login`).
client<llm> GoogleViaVertex {
    provider: google-ai,
    options: {
        model: "gemini-3.5-flash",
    },
}

function Greet(name: string) -> string {
    client: GoogleViaVertex
    prompt: `Say hello to ${name} in exactly one short sentence.`
}

// Native vertex-ai provider: same env-var resolution for location/project,
// same ADC auth — no options beyond the model.
client<llm> VertexNative {
    provider: vertex-ai,
    options: {
        model: "gemini-3.5-flash",
    },
}

function GreetVertex(name: string) -> string {
    client: VertexNative
    prompt: `Say hello to ${name} in exactly one short sentence.`
}

function main() -> string {
    Greet("BAML")
}

function main_vertex() -> string {
    GreetVertex("BAML on native Vertex")
}
