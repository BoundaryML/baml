function TemplateStringTestEcho(input: string) -> string {
  client GPT4o
  prompt #"
    {{ _.role("system") }}
    Echo the input string back to me verbatim.
    {{ _.role("user") }}
    {{ input }}
  "#
}

template_string TemplateStringForEchoTest(arg: int) ##"
  The number is {{ arg }}
"##

template_string TemplateStringForEchoTest2() ##"
  The number is 44
"##

test TemplateStringArgTest {
  functions [TemplateStringTestEcho]
  args {
   input: TemplateStringForEchoTest2()
  }
  @@assert( {{ this == "The number is 44" }} )
}
