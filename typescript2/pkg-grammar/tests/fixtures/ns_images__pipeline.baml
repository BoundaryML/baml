client AiGatewayImagenImageClient {
    provider: ai-gateway-images,
    options: {
        model: "google/imagen-4.0-fast-generate-001",
        api_key: env.AI_GATEWAY_API_KEY,
        n: 1,
    },
}

function generate_image(thing: string) -> image {
    client: AiGatewayImagenImageClient
    prompt: #"
        Create an image from this prompt:

        {{ thing }}

        {{ ctx.output_format }}
    "#
}
