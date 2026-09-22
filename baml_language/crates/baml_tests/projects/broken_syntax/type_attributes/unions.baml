// Attributes attach to declarations, never to types. An attribute ends the
// field's type, so anything after it that continues the type is a parse error.

class UA { x int }
class UB { x int }
class UC { x int }

class AttributeOnMiddleMember {
  field UA | UB @stream.done | UC
}

class AttributeOnFirstMember {
  field UA @alias("first") | UB | UC
}

class AttributeBeforeTrailingMembers {
  field UA | UB @stream.done | UC @alias("trailing")
}

class AttributeInsideParenthesesOnLastMember {
  field UA | (UB @alias("last"))
}

class AttributeInsideParenthesesOnInnerMember {
  field (UA | UB @alias("inner"))
}
