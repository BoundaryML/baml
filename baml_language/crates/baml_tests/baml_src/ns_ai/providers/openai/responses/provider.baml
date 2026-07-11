// OpenAI Responses API provider configuration and identity.

class OpenAiResponses {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
  append_output_schema: bool?,
  store: bool?,

  implements Provider {
    function display_name(self) -> string throws never { "openai-responses" }
    function render_shorthand(self) -> string throws never { "openai-responses/" + self.model }
  }
}
