// The demo agent's domain types (the BEP's travel agent).

class Flight {
    airline: string,
    price: float,
}

class Hotel {
    name: string,
    nightly: float,
}

class Itinerary {
    destination: string,
    days: int,
    flights: Flight[],
    hotel: Hotel?,
    daily_plan: string[],
}

//# Stand-in for ${ctx.output_format}: the schema the model must produce.

function itinerary_schema() -> string {
    `
        {
          "destination": string,
          "days": int,
          "flights": [{"airline": string, "price": float}],
          "hotel": {"name": string, "nightly": float} or null,
          "daily_plan": [string]
        }
    `
}
