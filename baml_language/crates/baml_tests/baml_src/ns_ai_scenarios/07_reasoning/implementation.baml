class s07_Diagnosis {
  root_cause: string,
  fix: string,
  confidence: float,
}

// Reasoning effort is provider configuration. The observable operation is still
// Generate and any reasoning trace belongs in Response.meta, not the value T.
class s07_FakeReasoningProvider {
  reply: string,
  effort: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let response = root.ai.FakeGenerateProvider {
        name: "reasoning-fake",
        reply: self.reply,
        attributes: { "reasoning_effort": self.effort },
      }.generate<T>(request);
      let blocks: root.ai.ModelBlock[] = [root.ai.ReasoningBlock {
        summary: "fake reasoning summary",
        redacted: false,
        provider_name: "reasoning-fake",
        metadata: null,
      }];
      response.meta.blocks = blocks;
      response
    }
  }
}
