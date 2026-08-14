class Hi {
    name string
}

client GeminiFlash = google.GoogleClient.new(
    model = "gemini-2.0-flash",
);

function FetchDatax(user_id: string) -> Hi {
    //# collect source material
    client: GeminiFlash

    prompt: `
        give short story 10 sentences
        make up data ${user_id}.

        ${ctx.output_format}
    `
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//     ╭─[headers_header_in_llm_function.baml:10:5]
//  10 │     //# collect source material
//     ·     ─────────────┬─────────────
//     ·                  ╰── header comments (`//#`) are only allowed in expression functions
//     ╰────
