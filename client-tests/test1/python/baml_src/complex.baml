function ClassifyTool {
  input UserInfo
  output ClassifyResponse
}

class ClassifyResponse {
  tool Tool
  assistant_response string
}

enum Tool {
  CodeInterpreter
  DrawImage
  GenerateText
}

class UserInfo {
  query string
  context string
}

impl<llm, ClassifyTool> v1 {
    client AZURE_GPT4
 
    override ImprovedResponse {
        should_improve @alias("ShouldImprove")
        @description(false if the response is already contextual and pleasant)
        @meta(fooz, "foo, bar, baz")

        improved_response @description(string if should_improve else null)
    }

    prompt #"
        Given a conversation with a user, classify the user's intent and generate a response.

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