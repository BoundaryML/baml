// Observable model output. These blocks are deliberately not the mutable
// source of truth for a future provider request; HarnessSessionToken is.
interface ModelBlock {
  function kind(self) -> string throws never
  function provider(self) -> string? throws never { null }
  function provider_metadata(self) -> json? throws never { null }
}

class TextBlock {
  text: string,
  provider_name: string?,
  metadata: json?,

  implements ModelBlock {
    function kind(self) -> string throws never { "text" }
    function provider(self) -> string? throws never { self.provider_name }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}

class ReasoningBlock {
  summary: string?,
  redacted: bool,
  provider_name: string,
  metadata: json?,

  implements ModelBlock {
    function kind(self) -> string throws never { "reasoning" }
    function provider(self) -> string? throws never { self.provider_name }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}

class ToolCallBlock {
  call: ToolCall,
  provider_name: string?,
  metadata: json?,

  implements ModelBlock {
    function kind(self) -> string throws never { "tool_call" }
    function provider(self) -> string? throws never { self.provider_name }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}

class ToolResultBlock {
  result: ToolResult,
  provider_name: string?,
  metadata: json?,

  implements ModelBlock {
    function kind(self) -> string throws never { "tool_result" }
    function provider(self) -> string? throws never { self.provider_name }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}

// Escape hatch for a losslessly retained provider block that has no portable
// BAML representation yet. Adapters can also add their own ModelBlock classes
// with an out-of-body implements block.
class ProviderBlock {
  block_kind: string,
  provider_name: string,
  metadata: json,

  implements ModelBlock {
    function kind(self) -> string throws never { self.block_kind }
    function provider(self) -> string? throws never { self.provider_name }
    function provider_metadata(self) -> json? throws never { self.metadata }
  }
}
