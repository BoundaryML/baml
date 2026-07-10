// 34 — cost is normalized metadata; batching is a collection lifecycle.
class s34_Ticket { category: string, severity: int, summary: string }
class s34_Price { input_per_million: float, output_per_million: float }

function s34_price(usage: root.ai.Usage, price: s34_Price) -> float throws never {
  let input = float.parse(usage.input_tokens.to_string()) catch (e) { _ => 0.0 };
  let output = float.parse(usage.output_tokens.to_string()) catch (e) { _ => 0.0 };
  (input * price.input_per_million + output * price.output_per_million) / 1000000.0
}

class s34_Metered {
  reply: string,
  input_tokens: int,
  output_tokens: int,
  implements root.ai.Provider {}
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.Response<T> {
        value: baml.sap.parse<T>(self.reply) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["metered parse"] },
        },
        meta: root.ai.Meta {
          provider: "metered", model: "fixture", request_id: null, finish_reason: "stop",
          usage: root.ai.Usage { input_tokens: self.input_tokens, output_tokens: self.output_tokens },
          attributes: {}, raw: null,
        },
      }
    }
  }
}

class s34_BatchProvider {
  implements root.ai.Provider { function display_name(self) -> string throws never { "batch-fixture" } }
  implements root.ai.Batching {
    function submit_batch<T>(
      self,
      requests: root.ai.Request<T>[],
      key_of: (root.ai.Request<T>, int) -> string throws never,
    ) -> root.ai.Batch<T> throws baml.errors.UnknownError {
      root.ai.Batch<T> { id: `batch-${requests.length()}`, owner: "batch-fixture" }
    }
  }
}
