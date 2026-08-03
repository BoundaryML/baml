// The user-facing surface: what `PlanTrip(...)` / `PlanTrip@session(...)`
// desugar to until the language has the sugar.

//# --- `PlanTrip@session(request = ...)` ---
function plan_trip_session(request: string, client: root.ai.Client, task_mode: bool = false) -> root.ai.Session<Itinerary, never> {
    let tb = root.ai.Toolbox { entries: [
        root.ai.tool(search_flights),
        root.ai.tool(search_hotels),
    ] };
    root.ai.new_session<Itinerary, never>(
        function_name = "PlanTrip",
        args_json = baml.json.to_string({ "request": request }),
        client = client,
        toolbox = tb,
        policy = root.ai.ToolLoop<never> { max_steps: 12, task_mode: task_mode },
        output_schema = itinerary_schema(),
    )
}

//# --- plain `PlanTrip(request)`: task mode ---
function plan_trip(request: string, client: root.ai.Client) -> Itinerary {
    let s = plan_trip_session(request, client, task_mode = true);
    match (s.run()) {
        let d: root.ai.Done<Itinerary> => d.result,     // typed all the way through
        let r: root.ai.Replied => throw baml.errors.InvalidArgument { message: `agent stopped without a result: ${r.message}` },
    }
}

//# Live run against OpenAI:  infisical run --env=test -- baml run -e 'demo()'
function live_client() -> root.ai.OpenAiClient {
    root.ai.OpenAiClient {
        model: "gpt-4o-mini",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        parser: PlanTripParser { },
        calls: 0,
    }
}

function demo() -> string {
    baml.json.to_string(plan_trip("2 weeks in Japan, mid-range budget", live_client()))
}
