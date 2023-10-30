class Foo {
  a int
  b string
}

function FooB {
  input InputType 
  output OutputType
} 

class InputType {
  a InputType2
  b bool  
}   

class OtherInput {
  d string
}
 
class InputType2 {
  c string
}

class OutputType {
  x Sentiment
  y bool
}


impl<llm, FooB> FooImpl{
  client MyClient
  prompt #"
    A {#input.a}!!    
    
    {// comment //}
    
    
    {#input.a.c}
    
    the rest of the prompt
    no-tab
      tab1
        tab2
    morespaces here
    {#input.a.c} {#input.b} hi there
    JSON:
    {#print_enum(Sentiment)}
    {#print_type(output)}
  "#
}
