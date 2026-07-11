// 34 — cost is normalized metadata; batching is a collection lifecycle.
class s34_Ticket { category: string, severity: int, summary: string }

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
