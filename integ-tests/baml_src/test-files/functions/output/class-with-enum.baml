enum EnumInClass {
  ONE
  TWO
}

class TestClassWithEnum {
  prop1 string
  prop2 EnumInClass
}

function FnOutputClassWithEnum {
  input string
  output TestClassWithEnum
}

impl<llm, FnOutputClassWithEnum> v1 {
  client GPT35
  prompt #"
    Return an array of a made up json blob that matches this schema:
    {#print_type(output)}

    Here are the values to use for enum:
    {#print_enum(EnumInClass)}
    ---

    JSON:
  "#
}