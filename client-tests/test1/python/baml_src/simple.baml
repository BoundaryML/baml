function Blah {
  input string
  output string
}  

impl<llm, Blah> v1 {  
  client ResilientGPT4
  prompt "hello there {#input}" 
}    