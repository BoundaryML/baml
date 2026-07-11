// OpenAI Chat Completions provider configuration and identity.

class OpenAi {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "openai" }
    function render_shorthand(self) -> string throws never { "openai/" + self.model }
  }
}
