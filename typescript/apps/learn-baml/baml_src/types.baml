// Context document provided to the assistant
class DocContext {
  title string @description("Document title")
  url string @description("URL path to the document")
  content string @description("Relevant content from the document")
  section string? @description("Section within the document")
}

// Chat message
class Message {
  role "user" | "assistant"
  text string
}

// Input to AskBaml function
class AskBamlInput {
  query string @description("User's question about BAML")
  context_docs DocContext[] @description("Relevant documentation chunks")
  prev_messages Message[] @description("Previous messages in conversation")
}

// Relevance rating for citations
enum Relevance {
  High @description("Directly answers the question")
  Medium @description("Related and helpful")
  Low @description("Tangentially related")
}

// Citation to include in response
class Citation {
  title string
  url string
  relevance Relevance
}

// Output from AskBaml function
class AskBamlOutput {
  answer string @description("Helpful answer to the user's question, using markdown")
  citations Citation[] @description("Documents cited in the answer")
  suggested_questions string[]? @description("Follow-up questions user might ask")
}
