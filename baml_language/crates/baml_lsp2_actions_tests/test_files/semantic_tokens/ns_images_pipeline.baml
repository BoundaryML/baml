client ImagePromptClient = google.GoogleClient.new(
    model = "gemini-2.5-flash",
    api_key = env.GOOGLE_API_KEY,
);

function generate_image_prompt(thing: string) -> string {
    client: ImagePromptClient
    prompt: `
        Write a detailed image-generation prompt for this subject:

        ${thing}

        ${ctx.output_format}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// ns_images_pipeline.baml:1:1 (keyword) len=6 "client"
// ns_images_pipeline.baml:1:26 (operator) len=1 "="
// ns_images_pipeline.baml:1:28 (namespace) [defaultLibrary] len=6 "google"
// ns_images_pipeline.baml:1:34 (namespace) [defaultLibrary] len=1 "."
// ns_images_pipeline.baml:1:35 (class) [defaultLibrary] len=12 "GoogleClient"
// ns_images_pipeline.baml:1:48 (method) len=3 "new"
// ns_images_pipeline.baml:2:5 (parameter) len=5 "model"
// ns_images_pipeline.baml:2:11 (operator) len=1 "="
// ns_images_pipeline.baml:2:13 (string) len=18 "\"gemini-2.5-flash\""
// ns_images_pipeline.baml:3:5 (parameter) len=7 "api_key"
// ns_images_pipeline.baml:3:13 (operator) len=1 "="
// ns_images_pipeline.baml:6:1 (keyword) len=8 "function"
// ns_images_pipeline.baml:6:10 (function) [declaration] len=21 "generate_image_prompt"
// ns_images_pipeline.baml:6:32 (parameter) [declaration] len=5 "thing"
// ns_images_pipeline.baml:6:39 (type) [defaultLibrary] len=6 "string"
// ns_images_pipeline.baml:6:50 (type) [defaultLibrary] len=6 "string"
// ns_images_pipeline.baml:7:5 (property) len=6 "client"
// ns_images_pipeline.baml:7:13 (variable) len=17 "ImagePromptClient"
// ns_images_pipeline.baml:8:5 (property) [declaration] len=6 "prompt"
// ns_images_pipeline.baml:8:13 (string) len=1 "`"
// ns_images_pipeline.baml:9:9 (string) len=5 "Write"
// ns_images_pipeline.baml:9:15 (string) len=1 "a"
// ns_images_pipeline.baml:9:17 (string) len=8 "detailed"
// ns_images_pipeline.baml:9:26 (string) len=16 "image-generation"
// ns_images_pipeline.baml:9:43 (string) len=6 "prompt"
// ns_images_pipeline.baml:9:50 (string) len=3 "for"
// ns_images_pipeline.baml:9:54 (string) len=4 "this"
// ns_images_pipeline.baml:9:59 (string) len=7 "subject"
// ns_images_pipeline.baml:9:66 (string) len=1 ":"
// ns_images_pipeline.baml:11:11 (parameter) len=5 "thing"
// ns_images_pipeline.baml:13:15 (property) len=13 "output_format"
// ns_images_pipeline.baml:14:5 (string) len=1 "`"
