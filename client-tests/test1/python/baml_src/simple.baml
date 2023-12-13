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

function ExtractVerbs {
  input (title: string, body: string)
  output string[]
}

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
}



impl<llm, ExtractVerbs> version1 {
  client GPT4
  prompt #"
    Extract the verbs from this paragraph:
    
    Title: {#input.title}
    ---
    {#input.body}
    --
    {// this is a comment inside a prompt! //}
    Return a {#print_type(output)}.

    Response:
  "#
}






