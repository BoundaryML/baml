class Gemini {
  model: string,
  api_key: string?,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "gemini" }
    function render_shorthand(self) -> string throws never { "google-ai/" + self.model }
  }
}
