// The round_robin provider allows you to distribute requests across multiple clients in a round-robin fashion. After each call, the next client in the list will be used.

client<llm> MyRoundRobinClient {
  provider round-robin
  options {
    strategy [
      GPT4o
      Gemini
    ]
  }
}
