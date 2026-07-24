client AiGatewayImagenImageClient {
    provider: ai-gateway-images,
    options: {
        model: "google/imagen-4.0-fast-generate-001",
        api_key: env.AI_GATEWAY_API_KEY,
        n: 1,
    },
}

function generate_image(thing: string) -> image {
    client: AiGatewayImagenImageClient
    prompt: #"
        Create an image from this prompt:

        {{ thing }}

        {{ ctx.output_format }}
    "#
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_images_pipeline.baml:1:1 (keyword) len=6 "client"
// ns_images_pipeline.baml:1:8 (struct) [declaration] len=26 "AiGatewayImagenImageClient"
// ns_images_pipeline.baml:2:5 (property) len=8 "provider"
// ns_images_pipeline.baml:3:5 (property) len=7 "options"
// ns_images_pipeline.baml:4:9 (property) len=5 "model"
// ns_images_pipeline.baml:4:16 (string) len=37 "\"google/imagen-4.0-fast-generate-001\""
// ns_images_pipeline.baml:5:9 (property) len=7 "api_key"
// ns_images_pipeline.baml:6:9 (property) len=1 "n"
// ns_images_pipeline.baml:6:12 (number) len=1 "1"
// ns_images_pipeline.baml:10:1 (keyword) len=8 "function"
// ns_images_pipeline.baml:10:10 (function) [declaration] len=14 "generate_image"
// ns_images_pipeline.baml:10:25 (parameter) [declaration] len=5 "thing"
// ns_images_pipeline.baml:10:32 (type) [defaultLibrary] len=6 "string"
// ns_images_pipeline.baml:10:43 (type) [defaultLibrary] len=5 "image"
// ns_images_pipeline.baml:11:5 (property) len=6 "client"
// ns_images_pipeline.baml:12:5 (property) [declaration] len=6 "prompt"
// ns_images_pipeline.baml:12:13 (string) len=1 "#"
// ns_images_pipeline.baml:12:14 (string) len=1 "\""
// ns_images_pipeline.baml:13:9 (string) len=6 "Create"
// ns_images_pipeline.baml:13:16 (string) len=2 "an"
// ns_images_pipeline.baml:13:19 (string) len=5 "image"
// ns_images_pipeline.baml:13:25 (string) len=4 "from"
// ns_images_pipeline.baml:13:30 (string) len=4 "this"
// ns_images_pipeline.baml:13:35 (string) len=6 "prompt"
// ns_images_pipeline.baml:13:41 (string) len=1 ":"
// ns_images_pipeline.baml:15:9 (string) len=1 "{"
// ns_images_pipeline.baml:15:10 (string) len=1 "{"
// ns_images_pipeline.baml:15:12 (string) len=5 "thing"
// ns_images_pipeline.baml:15:18 (string) len=1 "}"
// ns_images_pipeline.baml:15:19 (string) len=1 "}"
// ns_images_pipeline.baml:17:9 (string) len=1 "{"
// ns_images_pipeline.baml:17:10 (string) len=1 "{"
// ns_images_pipeline.baml:17:12 (string) len=3 "ctx"
// ns_images_pipeline.baml:17:15 (string) len=1 "."
// ns_images_pipeline.baml:17:16 (string) len=13 "output_format"
// ns_images_pipeline.baml:17:30 (string) len=1 "}"
// ns_images_pipeline.baml:17:31 (string) len=1 "}"
// ns_images_pipeline.baml:18:5 (string) len=1 "\""
// ns_images_pipeline.baml:18:6 (string) len=1 "#"
