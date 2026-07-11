class OpenAiStrict {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,

  implements Provider {
    function display_name(self) -> string throws never { "openai-strict" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }
}
