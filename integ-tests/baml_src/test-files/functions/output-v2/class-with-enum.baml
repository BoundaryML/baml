enum EnumInClass {
  ONE
  TWO
}

class TestClassWithEnum {
  prop1 string
  prop2 EnumInClass
}

function FnOutputClassWithEnum_V2(input: string  ) -> TestClassWithEnum {
  client GPT35
  prompt #"
    Return a made up json blob that matches this schema:
    {#print_type(output)}
    Here are the values to use for enum:
    {#print_enum(EnumInClass)}
    ---

    JSON:
  "#
}
