function LLMEcho(input: string) -> string {
    client GPT35
    prompt #"Echo exactly the input: {{input}}"#
}

fn EchoWorkflow() -> string {
    let input = "Hello, world!";
    LLMEcho(input)
}

test EchoWorkflow {
    functions [EchoWorkflow]
    args {}
}