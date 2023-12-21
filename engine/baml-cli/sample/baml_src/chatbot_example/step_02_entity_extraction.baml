// If you haven't already, first read step_01_classification.baml

// In this step, we will actually extract some bits of relevant information
// from the query.

// First, lets write the function.
function ExtractMeetingRequestInfo {
    // Functions can take multiple inputs, or even complex types.
    // see ./conversation.baml for more details on the Conversation type.
    input (convo: Conversation, now: string)
    output MeetingRequest

    default_impl simple
}

python#"
# We can call this function from python as follows:

from baml_client import baml
from datetime import datetime

def my_function(query: str) -> MeetingRequest:
    # baml.ExtractMeetingRequestInfo is strongly typed
    # and works with every typechecker.
    return baml.ExtractMeetingRequestInfo(
        query=query,
        now=datetime.now().isoformat()
    )
"#

class MeetingRequest {
    // BAML doesn't yet natively support date/time types, so we use a string.
     // We use @alias(when) because empircally works better with LLMs.
    time string @alias(when)
    @description(#"
        Either an exact time, or a relative time. Use ISO 8601 Duration Format
        when specifying a relative time (e.g. P1D for 1 day from now).
    "#)
    attendees Attendee[]
    topic string
}

class Attendee {
    name string
    email string
}

// Now similar to classification, we need to implement the function.
// Don't forget to open the playground to see what the final prompt looks like.
impl<llm, ExtractMeetingRequestInfo> simple {
    client Main

    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant
        information from the query.

        Context:
        ```
        Today is {#input.now}
        ```

        Conversation:
        ```
        {#input.convo.display}
        ```

        Output JSON:
        {#print_type(output)}

        JSON:
    "#
}

// That implmentation is fine, but it's not very useful as what happens if the user
// doesn't provide a time or any attendees? We can do something more robust.

class MeetingRequestPartial {
    time string? @alias(when)
    @description(#"
        Either an exact time, or a relative time. Use ISO 8601 Duration Format
        when specifying a relative time (e.g. P1D for 1 day from now).
    "#)
    duration string? @description(#"
        Use a ISO 8601 Duration Format (e.g. 1h for 1 hour).
    "#)
    attendees string[] @description(#"
        Names or preferably emails of attendees.
    "#)
    topic string?
    @description(#"
        What is the topic of the meeting?
    "#)
}

// Then, we could just have a function that only extracts partial
// information.

function ExtractMeetingRequestInfoPartial {
    input (convo: Conversation, now: string)
    output MeetingRequestPartial
}

impl<llm, ExtractMeetingRequestInfoPartial> v1 {
    client Main

    // We can use the same prompt as before.
    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant
        information from the query.

        Context:
        ```
        Today is {#input.now}
        ```

        Conversation:
        ```
        {#input.convo.display}
        ```

        Output JSON:
        {#print_type(output)}

        JSON:
    "#
}