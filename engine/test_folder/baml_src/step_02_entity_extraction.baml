// If you haven't already, first read step_01_classification.baml

// In this step, we will actually extract some bits of relevant information
// from the query.

// First, lets write the function.
function ExtractMeetingRequestInfo {
    input (query: string, now: string) // Functions can take multiple inputs
    output MeetingRequest
}

class MeetingRequest {
    when string // BAML doesn't yet support date/time types, but will.
    attendees Attendee[]
    topic string
}

class Attendee {
    name string
    email string
}

// Now similar to classification, we need to implement the function.
// Don't forget to open the playground to see what the final prompt looks like.
impl<llm, ExtractMeetingRequestInfo> v1 {
    client Main

    override MeetingRequest {
        when @description(#"
            Either an exact time, or a relative time. Use ISO 8601 Duration Format
            when specifying a relative time (e.g. P1D for 1 day from now).
        "#)
    }

    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant
        information from the query.

        Context:
        ```
        Today is {#input.now}
        ```

        Query:
        ```
        {#input.query}
        ```

        Output JSON:
        {#print_type(output)}

        JSON:
    "#
}

// That implmentation is fine, but it's not very useful as what happens if the user
// doesn't provide a time or any attendees? We can do better.

class MeetingRequestParital {
    when string?
    @description(#"
        Either an exact time, or a relative time. Use ISO 8601 Duration Format
        when specifying a relative time (e.g. P1D for 1 day from now).
    "#)
    attendees string[] @description(#"
        Names or preferably emails of attendees.
    "#)
    topic string?
    @description(#"
        What is the topic of the meeting?
    "#)
}

impl<llm, ExtractMeetingRequestInfo> robust {
    client Main

    // We'll use an adapter to make the function more robust.

    // This adapter allows us to use basic python that will convert from a
    // MeetingRequestParital to our output: MeetingRequest.
    adapter<MeetingRequestParital, output> python#"
        from utils import find_attendee_by_email

        attendees = [
            find_attendee_by_email(detail) if '@' in detail else Attendee(name=detail, email=None)
            for detail in input.attendees
        ]
        return MeetingRequest(
            when=input.when or "ASAP",
            attendees=attendees,
            topic=input.topic or "Meeting"
        )
    "#


    prompt #"
        Given a user is trying to schedule a meeting, extract the relevant
        information from the query.

        Context:
        ```
        Today is {#input.now}
        ```

        Query:
        ```
        {#input.query}
        ```

        Output JSON:
        {#print_type(output)}

        JSON:
    "#
}

{//

To use this function in your own code, you can do the following:

```python
from baml_client import baml
import datetime

async def my_giant_function(user_message: str) -> None:
    # You can do anything else you'd like prior:
    # ...
    intents = await baml.ClassifyIntent.get_impl('v1').run(user_message)

    if Intent.BookMeeting in intents:
        request = await baml.ExtractMeetingRequestInfo.get_impl('robust').run(
            query=user_message,
            now=datetime.now().isoformat()
        )
        print("Meeting request: {request.when} with {request.attendees} about {request.topic}")
    
    # Do the rest of your code here.
    return None # default

```

//}