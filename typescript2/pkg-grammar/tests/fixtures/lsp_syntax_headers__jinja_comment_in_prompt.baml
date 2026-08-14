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

    prompt #"
        {# You are the collection stage for the debugging workflow. Write a long story of 4 setnences. #}
        give short story 10 sentences
        {# Provide a compact JSON object (max 4 keys) called 'profile' that captures the most
        relevant public details you can infer for the candidate. Make one field 4 sentences long.  make up datea{{ user_id }}.

        {{ ctx.output_format }} #}
    "#
}
