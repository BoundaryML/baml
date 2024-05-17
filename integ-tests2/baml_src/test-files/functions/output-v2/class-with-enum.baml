enum EnumInClass2 {
  ONE
  TWO
}

class TestClassWithEnum2 {
  prop1 string
  prop2 EnumInClass
}

function FnOutputClassWithEnum_V2(input: string  ) -> TestClassWithEnum2 {
  client GPT35
  prompt #"
    Return a made up json blob that matches this schema:
    {{ ctx.output_format }}
    
    JSON:
  "#
}
