// The running example: the travel agent. Tools are plain functions with
// fake deterministic implementations, so offline tests and live runs share
// one fixture.

class Flight {
    airline: string,
    price_usd: float,
}

class Hotel {
    name: string,
    nightly_usd: float,
}

class Itinerary {
    flights: Flight[],
    hotels: Hotel[],
    total_cost_usd: float,
}

/// Search available flights for a route and month.
function search_flights(origin: string, destination: string, month: string) -> Flight[] {
    [Flight { airline: "ANA", price_usd: 950.0 }, Flight { airline: "ZipAir", price_usd: 620.0 }]
}

/// Search hotels in a city within a nightly budget.
function search_hotels(city: string, max_nightly_usd: float) -> Hotel[] {
    [
        Hotel { name: "Sakura Inn", nightly_usd: 140.0 },
        Hotel { name: "Ryokan Aoi", nightly_usd: 185.0 },
    ]
}

// The manual form of `PlanTrip@spec(trip_request = ...)` until the desugar
// exists: bind the arguments, build the Prompt closure, resolve the client.
function plan_trip_spec(
    trip_request: string,
    c: root.ai.Client,
) -> root.ai.FunctionSpec<Itinerary> {
    let args: map<string, unknown> = {};
    let _ = args.set("trip_request", trip_request);
    root.ai.FunctionSpec {
        spec_name: "PlanTrip",
        args: args,
        prompt_template: root.ai.Prompt {
            template: (output_format: string) -> {
                `
                    You are a travel agent. Plan the trip and answer with the final itinerary.
                    The brief: ${trip_request}
                    Use the tools to look up real options before answering.
                    ${output_format}
                `
            },
        },
        toolbox: root.ai.Toolbox.new([root.ai.tool(search_flights), root.ai.tool(search_hotels)]),
        default_client: c,
    }
}
