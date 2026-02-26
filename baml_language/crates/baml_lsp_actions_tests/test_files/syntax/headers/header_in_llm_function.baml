class Hi {
    name string
}

client<llm> VertexGCP {
    provider "vertex-ai"
    options {
        model "gemini-2.0-flash"
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

//----
//- diagnostics
// Error: Header comments (//#) are not allowed inside LLM functions
//     ╭─[ headers_header_in_llm_function.baml:13:5 ]
//     │
//  13 │     //# collect source material
//     │     ─────────────┬─────────────  
//     │                  ╰─────────────── Header comments (//#) are not allowed inside LLM functions
//     │ 
//     │ Note: Error code: E0010
// ────╯
