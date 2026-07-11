// 06 — output modality is the task's T. Image generation remains `Generate`
// because its observable lifecycle is still one request -> one typed response.

/// Deterministic image producer for the offline tier. The fixture already owns
/// typed data, so its generic bridge uses canonical JSON, not SAP (SAP is for
/// coercing LLM text output).
class s06_FixtureImage {
  base64_data: string,
  mime: string,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { "fixture-image" }
  }

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.UnknownError {
      let generated_image = baml.media.Image.from_base64(self.base64_data, self.mime);
      let encoded = baml.json.to_string<image>(generated_image) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["fixture image encode failed"] },
      };
      let value = baml.json.from_string<T>(encoded) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["fixture image type mismatch"] },
      };
      root.ai.Response<T> {
        value: value,
        meta: root.ai.Meta {
          provider: self.display_name(),
          model: "fixture-image",
          request_id: "fixture-image-1",
          finish_reason: "stop",
          usage: null,
          attributes: { "generation_kind": "image" },
          raw: null,
        },
      }
    }
  }

  implements root.ai.Capabilities {
    function structured_output(self) -> root.ai.Support throws never { root.ai.Support.No }
    function image_input(self) -> root.ai.Support throws never { root.ai.Support.No }
    function pdf_input(self) -> root.ai.Support throws never { root.ai.Support.No }
    function image_output(self) -> root.ai.Support throws never { root.ai.Support.Yes }
    function parallel_tools(self) -> root.ai.Support throws never { root.ai.Support.No }
    function max_input_tokens(self) -> int throws never { 0 }
  }
}
