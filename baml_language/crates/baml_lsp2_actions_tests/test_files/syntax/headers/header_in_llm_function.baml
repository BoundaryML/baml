class Hi {
    name string
}

client GeminiFlash = google.GoogleClient.new(
    model = "gemini-2.0-flash",
);

function FetchDatax(user_id: string) -> Hi {
    //# collect source material
    client GeminiFlash

    prompt `
        give short story 10 sentences
        make up data ${user_id}.

        ${ctx.output_format}
    `
}

//----
//- diagnostics
// <no-diagnostics-expected>
