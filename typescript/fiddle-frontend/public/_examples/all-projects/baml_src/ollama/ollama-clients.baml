client<llm> Llama3 {
  // See https://docs.boundaryml.com/docs/snippets/clients/providers/ollama
  // to learn more about how to configure this client
  //
  // Note that you should run ollama using `OLLAMA_ORIGINS='*' ollama serve`
  // and you'll also need to `ollama pull llama3` to use this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "llama3"
  }
}

client<llm> Mistral {
  // See https://docs.boundaryml.com/docs/snippets/clients/providers/ollama
  // to learn more about how to configure this client
  //
  // Note that you should run ollama using `OLLAMA_ORIGINS='*' ollama serve`
  // and you'll also need to `ollama pull mistral` to use this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "mistral"
  }
}

client<llm> Gemma2 {
  // See https://docs.boundaryml.com/docs/snippets/clients/providers/ollama
  // to learn more about how to configure this client
  //
  // Note that you should run ollama using `OLLAMA_ORIGINS='*' ollama serve`
  // and you'll also need to `ollama pull gemma2` to use this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "gemma2"
  }
}

client<llm> Phi3 {
  // See https://docs.boundaryml.com/docs/snippets/clients/providers/ollama
  // to learn more about how to configure this client
  //
  // Note that you should run ollama using `OLLAMA_ORIGINS='*' ollama serve`
  // and you'll also need to `ollama pull phi3` to use this client
  provider ollama
  options {
    base_url "http://localhost:11434/v1"
    model "phi3"
  }
}
