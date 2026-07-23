implements Streaming for OpenAi {
  function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
      throws baml.errors.UnknownError {
    _primitive_stream<TPartial, T>(
      _configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      request, "openai",
    )
  }
}
