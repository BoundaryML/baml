function ClassifyTool {
  input (query: string, context: string)
  output ClassifyResponse
}
   
class ClassifyResponse { 
  tool Tool[] @description(#"
    Any number of tools the user may want to use
  "#)  
  assistant_response string 
  
  pretty_message string @get(
    python#"
      import json

      thingy = []
      # iterate over stuff
      return f'You can use the following tools: {self.tool} {json.dumps(self.pretty_message2)}'
    "#
  )
   
    
  pretty_message2 string @get(
    python#"
      return f"Here is the assistant response: {self.assistant_response}"
    "#
  )
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