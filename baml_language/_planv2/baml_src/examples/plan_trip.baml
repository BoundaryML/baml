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

// The LLM function. Calling it directly runs the default ai.Agent loop;
// `PlanTrip@spec(trip_request = ...)` builds the bound, unrun
// ai.FunctionSpec<Itinerary> instead. The `client:` string names the default
// (resolved lazily, so building a spec never touches credentials); tests and
// live runs override it with `ai.Agent<Itinerary>.new(client = ...)`.
function PlanTrip(trip_request: string) -> Itinerary {
    client "openai/gpt-4o-mini"
    tools [search_flights, search_hotels]
    prompt `
        You are a travel agent. Plan the trip and answer with the final itinerary.
        The brief: ${trip_request}
        Use the tools to look up real options before answering.
        ${ctx.output_format}
    `
}
