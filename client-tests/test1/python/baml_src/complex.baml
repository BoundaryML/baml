function ClassifyTool {
  input (query: string, context: string)
  output ClassifyResponse
}

class ClassifyResponse { 
  tool Tool[] @description(#"
    Any number of tools the user may want to use
  "#)
  assistant_response string
}

enum Tool {
  @@alias(food)
  
  CodeInterpreter
  @alias(k1) 
  @description(Use this tool if the user is asking to compute something)

  DrawImage
  @alias(k2) 
  @description(Use this tool if the user is asking to draw something)

  GenerateText 
  @alias(k3) 
  @description(Use this if the tool is just asking for a simple answer)
}



impl<llm, ClassifyTool> v1 {
    client AZURE_GPT4

    override Tool {
      @@alias(tool)
    }


    prompt #"
        Given a conversation with a user, classify the user's intent {// the word intent works better than goal //} and generate a response.

        Userinfo:
        {#input.query}
 
        UserContext:
        {#input.context}

        {#print_enum(Tool)}

        Use this output format:
        {#print_type(output)}

        JSON:
    "#

}