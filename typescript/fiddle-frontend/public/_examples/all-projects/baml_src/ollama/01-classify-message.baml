enum OCategory {
    Refund
    CancelOrder
    TechnicalSupport
    AccountIssue
    Question
}

class OMessage {
  userName string
  message string
}

// inputs can be more complex than just a string
// This will be available as an enum in your Python and Typescript code.
// inputs can be more complex than just a string
function ClassifyMessageUsingOllama(message: OMessage) -> OCategory {
  client Llama3 
  // client Mistral
  // client Gemma2
  // client Phi3

  prompt #"
    Classify the following INPUT into ONE
    of categories listed.

    INPUT:
    ---
    {{ message.userName }}: {{ message.message }}
    ---

    {{ ctx.output_format }}

    Response:
  "#
}

test OllamaTest1 {
  functions [ClassifyMessageUsingOllama]
  args {
    message {
      userName "Alice"
      message "I can't access my account using my login credentials. I havent received the promised reset password email. Please help."
    }
  }
}