

function TestFnNamedArgsSingleBool(myBool: bool) -> string{
  client GPT35
  prompt #"
    Return this value back to me: {{myBool}}
  "#
} 

function TestFnNamedArgsSingleBoolExprFunc(myBool: bool) -> string{
  return "Hello, world!";
}

test TestFnNamedArgsSingleBool {
  functions [TestFnNamedArgsSingleBool]
  args {
    myBool true
  } 
}                  
         

     