// Simple completion prompt with no arguments
client GPT4 {
    provider openai
    options {
        model "gpt-4"
        api_key env.OPENAI_API_KEY
    }
}

function SimpleGreeting() -> string {
    client GPT4
    prompt #"
        Say hello to the world.
    "#
}
