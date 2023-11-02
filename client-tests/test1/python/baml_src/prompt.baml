function PromptTest {
  input string
  output Person[]
}

class Name {
  first string @alias(firsttt)
  @description("the first name")
  last string @alias(lst)
  @description(#"
    A last name is composed of
    1. The last name
    2. Some other thingies.
  "#)
}

enum PersonType {
  BAD
  GOOD
  NEUTRAL
}

class Person {
  personType PersonType
  //name Name @alias(nameOfPerson)
  //@description("some description")

  job string?
}

impl<llm, PromptTest> V1 {
  client MyClient
  prompt #"
    HELLO THERE
    {#input}

    Print in this:
    {#print_type(output)}
  "#
}