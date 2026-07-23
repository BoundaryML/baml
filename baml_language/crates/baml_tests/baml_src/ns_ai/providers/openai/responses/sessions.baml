implements Sessions for OpenAiResponses {
  function open_session(self, options: SessionOptions) -> Session throws never {
    OpenAiSession { owner: self, id: "session", previous_response_id: null }
  }

  function resume_session(self, token: SessionToken) -> Session throws baml.errors.UnknownError {
    if (token.provider != "openai-responses") {
      throw baml.errors.UnknownError { data: token, message: ["session token provider mismatch"] };
    }
    OpenAiSession { owner: self, id: token.id, previous_response_id: token.id }
  }
}
