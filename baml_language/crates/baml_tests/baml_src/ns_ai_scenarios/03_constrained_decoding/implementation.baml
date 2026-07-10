enum s03_Sentiment {
  Positive
  Negative
}

class s03_ChoiceConstraint {
  values: string[],
}

// Constrained decoding changes how generation happens, so it is a custom
// cross-provider capability with its own free driver.
interface s03_Constrained requires root.ai.Provider {
  function generate_constrained<T>(
    self,
    request: root.ai.Request<T>,
    constraint: s03_ChoiceConstraint,
  ) -> root.ai.Response<T> throws baml.errors.CallError | baml.errors.UnknownError
}

class s03_ConstrainedFixture {
  reply: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      BepV2FixtureGenerate {
        name: "constrained-fixture",
        reply: self.reply,
        attributes: {},
      }.generate<T>(request)
    }
  }

  implements s03_Constrained {
    function generate_constrained<T>(
      self,
      request: root.ai.Request<T>,
      constraint: s03_ChoiceConstraint,
    ) -> root.ai.Response<T> throws baml.errors.CallError | baml.errors.UnknownError {
      BepV2FixtureGenerate {
        name: "constrained-fixture",
        reply: self.reply,
        attributes: { "constraint_values": constraint.values },
      }.generate<T>(request)
    }
  }
}

function s03_run_constrained<T>(
  request: root.ai.Request<T>,
  constraint: s03_ChoiceConstraint,
) -> T throws baml.errors.CallError | baml.errors.UnknownError {
  match (request.provider) {
    let p: s03_Constrained => p.generate_constrained<T>(request, constraint).value,
    _ => throw baml.errors.Unsupported {
      message: "provider does not support constrained decoding: " + request.provider_name(),
    },
  }
}
