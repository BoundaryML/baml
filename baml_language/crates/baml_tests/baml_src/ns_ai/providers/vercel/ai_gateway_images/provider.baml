/// Image-only Vercel AI Gateway provider. It still implements `Generate`:
/// text prompt -> typed `image`/`image[]` is the same immediate interaction
/// shape, while the native primitive owns the gateway-specific wire codec.
class AiGatewayImages {
  model: string,
  api_key: string,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,

  implements Provider {
    function display_name(self) -> string throws never { "ai-gateway-images" }
    function render_shorthand(self) -> string throws never { "ai-gateway-images/" + self.model }
  }
}
