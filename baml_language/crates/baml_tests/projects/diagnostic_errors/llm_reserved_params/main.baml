function ReservedPromptContext(ctx: string, input: string) -> string {
    client: "openai/gpt-4o"
    prompt: `${input} ${ctx.output_format()}`
}
