function Fn() -> string {
  client "openai/gpt-4o"
  prompt #"Hi"#
}

class TopLevelClass {
  a string
}

enum TopLevelEnum {
  A
  B
}

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
