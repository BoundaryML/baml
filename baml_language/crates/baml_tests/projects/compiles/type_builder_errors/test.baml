// Test file for type_builder validation errors
// This tests the missing validations being implemented

function TypeBuilderFn(from_text: string) -> Resume { 
  client "openai/gpt-4o-mini"
  prompt #"Hello"#
  type_builder {
    class Foo {
      foo string
    }
    dynamic class Bar {
      bar int
    }
  }
}

class Resume {
  name string
}

test MultipleTypeBuilderBlocks {
  functions [TypeBuilderFn]
  type_builder {
    class Foo {
      foo string
    }
    dynamic class Bar {
      bar int
    }
  }
  type_builder {
    class A {
      a string
    }
    dynamic class B {
      b int
    }
  }
  args {
    from_text "Test"
  }
}

test SingleTypeBuilderBlock {
  functions [TypeBuilderFn]
  type_builder {
    class LocalClass {
      x string
    }
    dynamic class LocalDynamic {
      y int
    }
  }
  args {
    from_text "Test"
  }
}
