function MessageSimplifier {
    input Conversation
    output int?
}

impl<llm, MessageSimplifier> v1 {
  client AZURE_DEFAULT

  prompt #"
        Given a chat conversation between a human and ai
        simplify the most recent message from the human into a single sentence that includes all prior relevant context. Don't include any previously answered questions. 

        {#input}

        Most Recent Message:
        {#input}

        Simplified message:
        Human:
  "#
}
