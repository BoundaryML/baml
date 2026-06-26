// Per-million-token USD rate cards + cost estimation. Ports
// libs/bench_core/prices.py + prices.toml. Estimates for reporting only (the
// pipeline runs on a flat subscription, not metered API billing). The toml table
// is inlined as a match so there's no file IO at call time.

function prices_for(model: string) -> Prices? {
    match (model) {
        "claude-sonnet-4-6" => {
            Prices {
                input_per_million_usd: 3.0,
                output_per_million_usd: 15.0,
                cache_read_per_million_usd: 0.3,
                cache_write_per_million_usd: 3.75,
            }
        },
        "claude-haiku-4-5" => {
            Prices {
                input_per_million_usd: 1.0,
                output_per_million_usd: 5.0,
                cache_read_per_million_usd: 0.1,
                cache_write_per_million_usd: 1.25,
            }
        },
        "claude-opus-4-7" => {
            Prices {
                input_per_million_usd: 5.0,
                output_per_million_usd: 25.0,
                cache_read_per_million_usd: 0.5,
                cache_write_per_million_usd: 6.25,
            }
        },
        "claude-opus-4-8" => {
            Prices {
                input_per_million_usd: 5.0,
                output_per_million_usd: 25.0,
                cache_read_per_million_usd: 0.5,
                cache_write_per_million_usd: 6.25,
            }
        },
        _ => null,
    }
}

function compute_cost(
    input_tokens: int,
    output_tokens: int,
    cache_read_tokens: int,
    cache_write_tokens: int,
    prices: Prices,
) -> float {
    let total = input_tokens * prices.input_per_million_usd
        + output_tokens * prices.output_per_million_usd
        + cache_read_tokens * prices.cache_read_per_million_usd
        + cache_write_tokens * prices.cache_write_per_million_usd;
    return total / 1000000.0;
}
