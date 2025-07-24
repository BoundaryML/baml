class StreamBugResult {
    @@dynamic
    // response string
    // followup_questions string[]
}

function StreamBug() -> StreamBugResult {
    client GPT4
    prompt #"
        Please respond with a single string that has about 20 words and a list of 10 strings each with 3 words.
        {{ ctx.output_format }}
    "#
}

test StreamBug() {
    functions [StreamBug]
    args {}
}