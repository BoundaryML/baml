class Anthropic {
  model: string,
  api_key: string?,
  base_url: string?,
  max_tokens: int?,
  anthropic_version: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "anthropic" }
    function render_shorthand(self) -> string throws never { "anthropic/" + self.model }
  }
}
