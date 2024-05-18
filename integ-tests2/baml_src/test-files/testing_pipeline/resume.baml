class Resume {
    name string?
    email string?
    phone string?
    experience string[]
    education string[]
    skills string[]
}

function ExtractResume(resume: string) -> Resume {
    // client Resilient_ComplexSyntax
    client GPT4
    prompt #"
        Extract the following information from the resume:

        Resume:
        <<<<
        {{ resume }}
        <<<<

        {{ ctx.output_format }}

        JSON:
    "#
}
