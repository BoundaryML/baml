// Simple chat prompt with no arguments
client GPT4 {
    provider openai
    options {
        model "gpt-4"
        api_key env.OPENAI_API_KEY
    }
}

function SimpleChat() -> string {
    client GPT4
    prompt #"
        {{ _.chat("system") }}
        You are a helpful assistant.

        {{ _.chat("user") }}
        What is the capital of France?
    "#
}
