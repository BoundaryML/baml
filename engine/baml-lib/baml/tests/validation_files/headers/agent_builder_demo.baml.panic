# Agent Builder Demo Pipeline
function AgentBuilderDemo(user_query: string) -> string {
    ## Start Agent
    let start_context = StartAgent(user_query);

    ## Classification Agent
    let classifier_result = ClassifierAgent(start_context);

    ### Routing Decision
    let routed_response = if (classifier_result.output_parsed.classification == "flight_info") {
        #### Flight Agent Path
        let flight_response = FlightAgent(start_context);
        flight_response
    } else {
        #### Itinerary Agent Path
        let itinerary_response = ItineraryAgent(start_context);
        itinerary_response
    };

    ## Final Response
    routed_response
}

function StartAgent(input: string) -> string {
    input
}

function ClassifierAgent(input: string) -> struct {
    output_parsed: struct {
        classification: string,
    },
} {
    {
        output_parsed: {
            classification: "flight_info",
        },
    }
}

function FlightAgent(context: string) -> string {
    "flight-agent-response"
}

function ItineraryAgent(context: string) -> string {
    "itinerary-agent-response"
}
