// Legacy TypeBuilder syntax was removed by BEP-066 (runtime type
// construction is `baml.reflect` now). Every legacy form must surface a
// targeted E0098 "removed language feature" parse error instead of silently
// parsing (converted from the old compiles/type_builder_test project).

function Fn(from_text: string) -> string {
  client: "openai/gpt-4o"
  prompt: `Hi`
}

// Old-style test with a `type_builder` block: one E0098 for the block.
test MyTest {
  functions [Fn]
  type_builder {
    class InnerClass {
      b int
    }
    dynamic class DynamicInner {
      c string
    }
    enum InnerEnum {
      X
      Y
    }
    dynamic enum DynamicEnum {
      P
      Q
    }
    type MyAlias = string | int
  }
  args {
    from_text "test"
  }
}

// `type_builder` as the FIRST key still routes the test through old-style
// config-block parsing (no `functions` before it): one E0098.
test TypeBuilderFirst {
  type_builder {
    class FirstInner {
      a int
    }
  }
  functions [Fn]
  args {
    from_text "test"
  }
}

// Colon form inside an old-style test: also one E0098.
test ColonForm {
  functions [Fn]
  type_builder: {
    class ColonInner {
      x int
    }
  }
  args {
    from_text "test"
  }
}

// `dynamic class` / `dynamic enum` directly inside a test config block.
test DirectDynamic {
  functions [Fn]
  dynamic class DirectDynamicClass {
    y string
  }
  dynamic enum DirectDynamicEnum {
    A
  }
  args {
    from_text "test"
  }
}

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
