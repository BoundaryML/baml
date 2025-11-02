// Test parser recovery with partial/incomplete input
class Partial {
  field1 str  // Incomplete type name
  field2  // Missing type entirely
  field3 string @  // Incomplete attribute
  field4 string @alias(  // Unclosed attribute
  field5 string  // This should parse correctly
}

function Incomplete(  // Missing parameters closing
  param1 string

function NextFunction() -> string {  // Should still parse this
  return "parsed"
}

enum  // Missing enum name
  VALUE1
  VALUE2
}

enum ValidEnum {  // Should parse this
  OPTION1
  OPTION2
}
