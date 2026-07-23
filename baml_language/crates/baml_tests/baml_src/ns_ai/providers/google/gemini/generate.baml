implements Generate for Gemini {
  function generate<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError {
    _primitive_generate<T>(
      _configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      request, "gemini", self.model,
    )
  }
}
