enum s03_Sentiment {
  Positive
  Negative
}

class s03_ConstrainedFixture {
  reply: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.FixtureGenerate {
        name: "constrained-fixture",
        reply: self.reply,
        attributes: {},
      }.generate<T>(request)
    }
  }

  implements root.ai.Constrained {
    function generate_constrained<T>(
      self,
      request: root.ai.Request<T>,
      constraint: root.ai.ChoiceConstraint,
    ) -> root.ai.Response<T> throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.FixtureGenerate {
        name: "constrained-fixture",
        reply: self.reply,
        attributes: { "constraint_values": constraint.values },
      }.generate<T>(request)
    }
  }
}
