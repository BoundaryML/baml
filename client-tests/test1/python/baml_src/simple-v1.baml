function HaikuAlpha(product_review: string) -> string {
    client ResilientGPT4

    prompt #"
        {{ _.chat("system")}}

        You are a poet writing in the style of 17th century German haikus.

        {{ _.chat("user")}}

        Write a haiku about {{ ctx.client.provider }}.

        JSON:
    "#

}  
