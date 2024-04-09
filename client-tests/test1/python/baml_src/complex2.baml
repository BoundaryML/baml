function BClassifyTool(query: string, context: string) -> ClassifyResponse {
    client AZURE_GPT4

    prompt #"
        {{ query }

        UserContext:
        
        {{ context }}
  
        Use this output format:
        {{ ctx.output_schema }}

        JSON:
    "#

}  
