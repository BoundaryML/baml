class Hi {
    name string
}

client<llm> VertexGCP {
    provider vertex-ai
    options {
        model gemini-2.0-flash
    }
}

function FetchDatax(user_id: string) -> Hi {
    //# collect source material
    client VertexGCP

    prompt #"
        give short story {{ user_id }}
        {{ ctx.output_format }}
    "#
}
