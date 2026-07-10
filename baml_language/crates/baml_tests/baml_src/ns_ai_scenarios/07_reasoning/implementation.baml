class s07_Diagnosis {
  root_cause: string,
  fix: string,
  confidence: float,
}

// Reasoning effort is provider configuration. The observable operation is still
// Generate and any reasoning trace belongs in Response.meta, not the value T.
class s07_ReasoningFixture {
  reply: string,
  effort: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      BepV2FixtureGenerate {
        name: "reasoning-fixture",
        reply: self.reply,
        attributes: { "reasoning_effort": self.effort },
      }.generate<T>(request)
    }
  }
}
