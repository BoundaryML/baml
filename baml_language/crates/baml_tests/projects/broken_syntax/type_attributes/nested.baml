// Attributes cannot appear inside a nested type: not in parentheses, not in
// generic arguments.

class NFoo { x int }
class NBar { x int }

class AttributeInsideParentheses {
  field (NFoo @alias("inner"))[]
}

class StreamAttributeInsideParentheses {
  field (NFoo @stream.done)[]
}

class AttributeInsideGenericArgument {
  field map<NFoo @alias("key"), NBar>
}

class StreamAttributeInsideGenericArgument {
  field map<NFoo @stream.done, NBar>
}

class AttributeInsideParenthesizedUnion {
  field (NFoo @alias("inner") | NBar)[]
}

class AttributeInsideParenthesesWithTrailingAttribute {
  field (NFoo @alias("inner"))[] @stream.done
}

class AttributeInsideAndAfterParentheses {
  field (NFoo @alias("inner")) @alias("outer")
}

class AttributeInsideNestedGenericArgument {
  field map<string, map<NFoo @alias("key"), NBar>>
}
