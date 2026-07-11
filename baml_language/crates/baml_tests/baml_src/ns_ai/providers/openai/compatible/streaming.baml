implements Streaming for OpenAiCompatible {
  function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
      throws baml.errors.UnknownError {
    _primitive_stream<TPartial, T>(
      _configured_primitive(
        self.render_shorthand(), self.model, self._api_key(), self.base_url,
        self._headers(), self.extra_body,
      ),
      request, "openai-compatible",
    )
  }
}
