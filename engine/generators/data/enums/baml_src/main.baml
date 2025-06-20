enum TestEnum {
  A @alias("k1") @description(#"
    User is angry
  "#)
  B @alias("k22") @description(#"
    User is happy
  "#)
  // tests whether k1 doesnt incorrectly get matched with k11
  C @alias("k11") @description(#"
    User is sad
  "#)
  D @alias("k44") @description(
    User is confused
  )
  E @description(
    User is excited
  )
  F @alias("k5") // only alias
  
  G @alias("k6") @description(#"
    User is bored
    With a long description
  "#)
   
  @@alias("Category")
}

function FnTestAliasedEnumOutput(input: string) -> TestEnum {
  client "openai/gpt-4o"
  prompt #"
    Classify the user input into the following category
      
    {{ ctx.output_format }}

    {{ _.role('user') }}
    {{input}}

    {{ _.role('assistant') }}
    Category ID:
  "#
}

function ConsumeTestEnum(input: TestEnum) -> TestEnum {
  client "openai/gpt-4o"
  prompt #"
    Return back to me verbatim:
    
    {{ input }}
  "#
}

test FnTestAliasedEnumOutput {
  functions [FnTestAliasedEnumOutput]
  args {
    input "mehhhhh"
  }
}