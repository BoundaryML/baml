class TokenPrice {
    input_per_million: float,
    output_per_million: float,
}

function estimated_cost(usage: ai.Usage, price: TokenPrice) -> float throws never {
    let input = float.parse(usage.input_tokens.to_string()) catch (e) {
        _ => 0.0
    };
    let output = float.parse(usage.output_tokens.to_string()) catch (e) {
        _ => 0.0
    };
    (input * price.input_per_million + output * price.output_per_million) / 1000000.0
}

function aggregate_usage(responses: ai.ResponseMetadata[]) -> ai.Usage throws never {
    let input = 0;
    let output = 0;
    let cached = 0;
    let reasoning = 0;
    //# Aggregate usage from every provider response
    for (let meta in responses) {
        //## Add available token counters
        match (meta.usage) {
            let usage: ai.Usage => {
                input = input + usage.input_tokens;
                output = output + usage.output_tokens;
                cached = cached + (usage.cached_input_tokens ?? 0);
                reasoning = reasoning + (usage.reasoning_tokens ?? 0);
            },
            null => {},
        }
    }
    ai.Usage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        reasoning_tokens: reasoning,
    }
}
