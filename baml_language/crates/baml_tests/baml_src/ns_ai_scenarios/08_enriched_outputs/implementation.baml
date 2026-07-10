class s08_Sentiment {
  label: string,
}

class s08_Answer {
  text: string,
}

class s08_EnrichedFixture {
  reply: string,
  source_url: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      BepV2FixtureGenerate {
        name: "enriched-fixture",
        reply: self.reply,
        attributes: { "source_url": self.source_url, "confidence": 0.97 },
      }.generate<T>(request)
    }
  }
}
