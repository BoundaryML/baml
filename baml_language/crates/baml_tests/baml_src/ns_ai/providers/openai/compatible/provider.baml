class OpenAiCompatible {
  base_url: string,
  model: string,
  auth: Auth,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  function _api_key(self) -> string? throws never {
    match (self.auth) {
      let bearer: BearerAuth => bearer.token,
      _ => null,
    }
  }

  function _headers(self) -> map<string, string> throws never {
    let headers: map<string, string> = self.extra_headers ?? {};
    match (self.auth) {
      let custom: HeaderAuth => { let _ = headers.set(custom.name, custom.value); },
      _ => {},
    }
    headers
  }

  implements Provider {
    function display_name(self) -> string throws never { "openai-compatible" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }
}
