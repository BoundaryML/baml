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
  @@alias(tools)
  
  CodeInterpreter
  @alias(k1) 
  @description(Use this tool if the user is asking to compute something)

  DrawImage
  @alias(k2) 
  @description(Use this tool if the user is asking to draw something)

  GenerateText 
  @alias(k3)
  @description(Use this tool if the user is asking to generate text)
}

 
 // Open this in the playground to see what {#print_enum()} and {#print_type()} are doing
impl<llm, ClassifyTool> v1 {
    client AZURE_GPT4

    // you can also declare aliases for each impl
    override ClassifyResponse {
      assistant_response @alias(foo)
      @description(#"
        The response, in a polite tone.
      "#)
    }

    override Tool {
      @@alias(TOOL LIST) // make print_enum list all the enum tools under a heading called TOOL LIST for this impl only
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