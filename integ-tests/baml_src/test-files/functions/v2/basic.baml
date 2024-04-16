class Resume {
    name string
    email string
    phone string
    experience string[]
    education string[]
    skills string[]
}

function ExtractResume(resume: string) -> Resume {
    client Ollama
    prompt #"
        Extract the following information from the resume:

        Resume:
        <<<<
        {{ resume }}
        <<<<

        Output JSON schema:
        {{ ctx.output_schema }}

        JSON:
    "#
}