// You can use the fallback provider to add more resilliancy to your application.

// A fallback will attempt to use the first client, and if it fails, it will try the second client, and so on.

// https://docs.boundaryml.com/docs/snippets/clients/fallback

client<llm> SuperDuperClient {
  provider fallback
  options {
    // clients from clients.baml
    strategy [
      GPT4
      GPT4o
      Gemini
    ]
  }
}
