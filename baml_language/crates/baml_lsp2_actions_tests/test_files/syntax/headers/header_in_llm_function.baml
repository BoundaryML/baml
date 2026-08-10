class Hi {
    name string
}

client<llm> VertexGCP {
    provider "vertex-ai"
    options {
        model "gemini-2.0-flash"
        location "us-central1"
    }
}

function FetchDatax(user_id: string) -> Hi {
    //# collect source material
    client VertexGCP

    prompt `

        give short story 10 sentences

    `
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//     ╭─[headers_header_in_llm_function.baml:14:5]
//  14 │     //# collect source material
//     ·     ─────────────┬─────────────
//     ·                  ╰── header comments (`//#`) are not allowed inside LLM functions
//     ╰────
