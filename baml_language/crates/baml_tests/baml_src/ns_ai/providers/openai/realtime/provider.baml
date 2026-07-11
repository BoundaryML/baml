class OpenAiRealtime {
  model: string?,
  voice: string,
  api_key: string,

  implements Provider {
    function display_name(self) -> string throws never { "openai-realtime" }
    function render_shorthand(self) -> string throws never {
      "openai/" + (self.model ?? "gpt-realtime")
    }
  }
}
