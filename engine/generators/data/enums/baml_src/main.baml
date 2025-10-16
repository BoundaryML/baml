enum TestEnum {
  Angry @alias("k1") @description(#"
    User is angry
  "#)
  Happy @alias("k2") @description(#"
    User is happy
  "#)
  // tests whether k1 doesnt incorrectly get matched with k11
  Sad @alias("k3") @description(#"
    User is sad
  "#)
  Confused @description(
    User is confused
  )
  Excited @alias("k5") @description(
    User is excited
  )
  Exclamation @alias("k6") // only alias
  
  Bored @alias("k7") @description(#"
    User is bored
    With a long description
  "#)
   
  @@alias("Category")
}

function FnTestAliasedEnumOutput(input: string) -> TestEnum {
  client "openai/gpt-4o-mini"
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
  client "openai/gpt-4o-mini"
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