// Tools are plain functions: signature + docstring = the schema the
// model sees (via reflection in root.ai.Toolbox).

/// Search available flights between two cities.
function search_flights(origin: string, dest: string) -> Flight[] {
    [Flight { airline: "ANA", price: 890.0 }, Flight { airline: "United", price: 940.0 }]
}

/// Find hotels in a city under a nightly budget.
function search_hotels(city: string, max_nightly: float) -> Hotel[] {
    [Hotel { name: `${city} Ryokan`, nightly: max_nightly - 10.0 }]
}
