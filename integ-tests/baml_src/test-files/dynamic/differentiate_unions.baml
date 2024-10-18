class OriginalA {
  value int
}

class OriginalB {
  value int
  @@dynamic
}

function DifferentiateUnions() -> OriginalA | OriginalB {
  client "openai/gpt-4o-mini"
  prompt #"
    Create a data model that represents the latter of the two classes.

    {{ ctx.output_format  }}
  "#
}