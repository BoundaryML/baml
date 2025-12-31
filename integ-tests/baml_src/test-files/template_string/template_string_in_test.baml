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
  The {{ SubString() }} is {{ arg }}
"##

template_string SubString() #"
  FOO
"#

test TemplateStringArgTest {
  functions [TemplateStringTestEcho]
  args {
   input: TemplateStringForEchoTest(43)
  }
  @@assert( {{ this == "The FOO is 43" }} )
}
