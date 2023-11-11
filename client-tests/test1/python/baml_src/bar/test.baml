class ProposedMessage {
    thread Conversation
    generated_response string 
}

enum  Sentiment {
    Positive
    Negative
    Neutral
}

class ImprovedResponse {
    should_improve bool
    improved_response string?
    field Sentiment
}

function MaybePolishText {
    input ProposedMessage
    output ImprovedResponse
}

function TextPolisher {
    input ProposedMessage
    output string
} 
   
 

impl<llm, MaybePolishText> v1 {
    client AZURE_GPT4
 
    override ImprovedResponse {
        should_improve @alias("ShouldImprove")
        @description(false if the response is already contextual and pleasant)
        @meta(fooz, "foo, bar, baz")

        improved_response @description(string if should_improve else null)
    }

    prompt #"
        Given a conversation with a resident, consider improving the response previously shown.

        Good responses are amiable and direct.

        Do not use {// affirmative //} or negative unless the question is a yes or no question.

        Thread until now:
        {#input.thread}

        Previous Response: {#input.generated_response}

        {#print_enum(Sentiment)}

        {// The word format is required. don't remove it //}
        Output JSON Format:
        {#print_type(output)}

        JSON:
    "#
}
