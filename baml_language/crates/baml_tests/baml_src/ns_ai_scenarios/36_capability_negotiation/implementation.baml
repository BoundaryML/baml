// 36 — interfaces answer operation support; descriptors answer graded support.
class s36_Report { subject: string, verdict: string }

class s36_DescribedFixture {
  reply: string,
  support: root.ai.Support,
  implements root.ai.Provider { function display_name(self) -> string throws never { "described-fixture" } }
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.Response<T> {
        value: baml.sap.parse<T>(self.reply) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["descriptor fixture parse"] },
        },
        meta: root.ai.Meta {
          provider: "described-fixture", model: null, request_id: null, finish_reason: "stop",
          usage: null, attributes: {}, raw: null,
        },
      }
    }
  }
  implements root.ai.Capabilities {
    function structured_output(self) -> root.ai.Support throws never { self.support }
    function image_input(self) -> root.ai.Support throws never { root.ai.Support.No }
    function parallel_tools(self) -> root.ai.Support throws never { root.ai.Support.No }
    function max_input_tokens(self) -> int throws never { 4096 }
  }
}

function s36_structured_label(provider: root.ai.Provider) -> string throws never {
  match (provider) {
    let c: root.ai.Capabilities => match (c.structured_output()) {
      root.ai.Support.Yes => "yes",
      root.ai.Support.No => "no",
      root.ai.Support.Conditional => "conditional",
    },
    _ => "unknown",
  }
}
