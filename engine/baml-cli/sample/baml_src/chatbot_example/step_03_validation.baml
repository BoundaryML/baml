// We can now validate if the prompt is valid.
// We will declare a function that takes in the current information gathered from the user (MeetingRequestPartial) and returns a Validation.

function GetNextQuestion {
    input MeetingRequestPartial
    output Validation
}

class Validation {
    requirements_complete bool
    // A question to ask the user to fill in the rest of the MeetingRequestPartial that may
    // be missing
    follow_up_question string?
}

impl<llm, GetNextQuestion> v1 {
    client Main

    override Validation {
        requirements_complete @alias(complete) @description(#"
            True if there is enough information is filled out in the input. I don't need any more fields.
        "#)
        follow_up_question @description(#"
            If not complete, what question should be asked next? Try to gather as much information as possible in one question as this is answered in an email.
        "#)
    }

    prompt #"
        Given the partial requirements, are the requirements complete?

        Input:
        ```
        {// Remember the input to this function is the current information gathered from the user //}
        {#input}
        ```

        Output JSON:{// JSON works better than format here.//}
        {#print_type(output)}

        JSON: 
    "#
}
