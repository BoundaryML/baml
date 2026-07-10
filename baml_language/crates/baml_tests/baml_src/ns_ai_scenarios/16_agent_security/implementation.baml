class s16_PageFacts {
  topic: string,
  key_facts: string[],
}

class s16_Tainted<T> {
  value: T,
  source: string,

  function from(value: T, source: string) -> s16_Tainted<T> {
    s16_Tainted<T> { value: value, source: source }
  }

  function is_tainted(self) -> bool throws never { true }
}

// Quarantine exposes only Generate even when the inner provider also has Tools.
// Untrusted content can be read, but it cannot gain a tool-execution surface.
class s16_Quarantined {
  inner: root.ai.Provider,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.run_with_meta<T>(request.for_provider(self.inner))
    }
  }
}

function s16_host_allowed(allow_hosts: string[], url: string) -> bool throws never {
  for (let host in allow_hosts) {
    if (url.includes(host)) { return true; }
  }
  false
}
