// Legacy TypeBuilder syntax was removed by BEP-066 (runtime type
// construction is `reflect` now). Every legacy form must surface a
// targeted E0098 "removed language feature" parse error instead of silently
// parsing (converted from the old compiles/type_builder_test project).

// `type_builder` inside an LLM function body.
function LlmWithTypeBuilder(from_text: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `Hello`
  type_builder {
    class LlmInner {
      foo string
    }
    dynamic class LlmDynamic {
      bar int
    }
  }
}

// Top-level `type_builder` block.
type_builder {
  class TopLevelInner {
    z int
  }
}

// Top-level `dynamic class`.
dynamic class TopLevelDynamic {
  w string
}
