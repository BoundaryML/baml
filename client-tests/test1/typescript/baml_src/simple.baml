function Blah {
  input string
  output string

  default_impl v1
}  

impl<llm, Blah> v1 {
  client ResilientGPT4
  prompt #"hello there {#input}"#
}  

impl<llm, Blah> v2 {  
  client ResilientGPT4
  prompt #"whats your name {#input}"#
}