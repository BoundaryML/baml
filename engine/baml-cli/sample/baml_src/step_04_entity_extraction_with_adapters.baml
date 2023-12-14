// Adapters are an advanced concept (not required), but allow us to 
// add an abstraction layer to our functions so our AI functions
// can be more flexible without having to change the interface to
// application logic.

// Applications include:
// - Adding a default value to a field
// - RAG
// - Dynamic Few-Shot Learning

// Here's we'll use an adapter to change ExtractMeetingRequestInfo
// to allow for a partial meeting request instead of needing a new function
// ExtractMeetingRequestInfoPartial

// Read our guide: https://docs.boundaryml.com/v3/guides/entity_extraction/level4

impl<llm, ExtractMeetingRequestInfo> robust {
    client Main

    // We'll use an adapter to make the function more robust.

    // This adapter allows us to use basic python that will convert from a
    // MeetingRequestParital to our output: MeetingRequest.
    adapter<MeetingRequestPartial, output> python#"
        # This is a pure python function we're importing from a python file
        # we've written (./py_baml_example/utils.py)

        from py_baml_example.utils import find_attendee_by_email

        attendees = [
            find_attendee_by_email(detail) if '@' in detail else Attendee(name=detail, email="unknown")

            # arg is a special variable that contains a type of the
            # previous step (in this case, the LLM).
            for detail in arg.attendees
        ]
        return MeetingRequest(
            time=arg.time or "ASAP",
            attendees=attendees,
            topic=arg.topic or "Meeting"
        )
    "#

    // We can use the same prompt as before, but due to the adapter, we will
    // compile the prompt differently.
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

python#"
# We can call this function from python as follows:

from baml_client import baml

# if we change the default_impl to 'robust', we can use the same function
def my_function(query: str) -> MeetingRequest:
    return baml.ExtractMeetingRequestInfo(
        query=query,
        now=datetime.now().isoformat()
    )
    # Or we can be explicit:
    # return baml.ExtractMeetingRequestInfo.get_impl('robust').run(
    #     query=query,
    #     now=datetime.now().isoformat()
    # )
"#

// alternatively, we could just have a function that only extracts partial
