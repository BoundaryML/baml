enum EnumInClass {
  ONE
  TWO
}

class TestClassWithEnum {
  prop1 string
  prop2 EnumInClass
}

function FnOutputClassWithEnum(input: string) -> TestClassWithEnum {
  client GPT35
  prompt #"
    Return a made up json blob that matches this schema:
    {{ctx.output_format}}
    ---

    JSON:
  "#
}

test FnOutputClassWithEnum {
  functions [FnOutputClassWithEnum]
  args {
    input "example input"
  }
}
