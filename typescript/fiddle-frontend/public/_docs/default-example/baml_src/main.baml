function SimpleGreeting() -> string {
  client "openai/gpt-4o"
  prompt #"
    You are a friendly assistant.
    Respond with a simple greeting.
  "#
}

test SimpleGreetingTest {
  functions [SimpleGreeting]
  args {}
} 