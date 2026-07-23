implements Generate for OpenAiCompatible {
  function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
    _primitive_generate<T>(
      _configured_primitive(
        self.render_shorthand(), self.model, self._api_key(), self.base_url,
        self._headers(), self.extra_body,
      ),
      request, "openai-compatible", self.model,
    )
  }
}
