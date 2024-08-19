client<llm> Llama3 {
  // See https://www.boundaryml.com/blog/ollama-structured-output
  // to learn more about how to use 
  // Make sure to run 'ollama pull llama3'
  // before using this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "llama3"
  }
}

client<llm> Mistral {
  // See https://www.boundaryml.com/blog/ollama-structured-output
  // to learn more about how to use 
  // Make sure to run 'ollama pull mistral'
  // before using this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "mistral"
  }
}

client<llm> Gemma2 {
  // See https://www.boundaryml.com/blog/ollama-structured-output
  // to learn more about how to use 
  // Make sure to run 'ollama pull gemma2'
  // before using this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "gemma2"
  }
}

client<llm> Phi3 {
  // See https://www.boundaryml.com/blog/ollama-structured-output
  // to learn more about how to use 
  // Make sure to run 'ollama pull phi3'
  // before using this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "phi3"
  }
}
