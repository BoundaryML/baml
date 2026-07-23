class s08_Sentiment {
  label: string,
}

class s08_Answer {
  text: string,
}

class s08_FakeEnrichedProvider {
  reply: string,
  source_url: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let response = root.ai.FakeGenerateProvider {
        name: "enriched-fake",
        reply: self.reply,
        attributes: { "source_url": self.source_url, "confidence": 0.97 },
      }.generate<T>(request);
      response.meta.citations = [root.ai.Citation {
        url: self.source_url,
        title: "fake source",
      }];
      response
    }
  }
}
