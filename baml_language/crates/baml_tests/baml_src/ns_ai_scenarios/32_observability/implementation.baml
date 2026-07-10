// 32 — metadata is returned once with the value; tracing is a provider wrapper.
class s32_Metered {
  name: string,
  reply: string,
  input_tokens: int,
  output_tokens: int,

  implements root.ai.Provider {
    function display_name(self) -> string throws never { self.name }
  }
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.Response<T> {
        value: baml.sap.parse<T>(self.reply) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["metered fixture parse"] },
        },
        meta: root.ai.Meta {
          provider: self.name, model: "fixture", request_id: null, finish_reason: "stop",
          usage: root.ai.Usage { input_tokens: self.input_tokens, output_tokens: self.output_tokens },
          attributes: {}, raw: null,
        },
      }
    }
  }
}
