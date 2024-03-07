class ProposedMessage {
    thread Conversation
    generated_response string 
}


enum  Sentiment {
    Positive
    Negative
    Neutral
}

class Haiku {
    haiku string
}
 
class ImprovedResponse {
    should_improve bool
    improved_response string? 
    field Sentiment
} 

 
function MaybePolishText {
    input ProposedMessage
    output string
    default_impl v3
}
 
// function TextPolisher {
//     input ProposedMessage
//     output string
// }

// impl<llm, TextPolisher> v1 {
//     client AZURE_GPT4
//     prompt #"
//         Write a haiku about:
//         ```input
//         {#input}
//         ```

//         JSON:
//     "#
// }
 
impl<llm, MaybePolishText> v1 {
    client AZURE_GPT4
    adapter<string, output> python#"
        return "hi"
    "#

 
    // override ImprovedResponse {
    //     should_improve @alias("ShouldImprove")
    //     @description(false if the response is already contextual and pleasant)
    //     @meta(fooz, "foo, bar, baz")

    //     improved_response @description(string if should_improve else null)
    // } 

    prompt #"
        Write a haiku about {#input.generated_response}
        
        ignore
        """
        ignore

        add it to this json schema and return it
        {#print_type(output)}
        
        JSON:
    "#
 
}

impl<llm, MaybePolishText> v2 {
    client AZURE_GPT4
 
    // override ImprovedResponse {
    //     should_improve @alias("ShouldImprove")
    //     @description(false if the response is already contextual and pleasant)
    //     @meta(fooz, "foo, bar, baz")

    //     improved_response @description(string if should_improve else null)
    // }

    prompt #"
        Given a conversation with a resident, consider improving the response previously shown.

        Good responses are amiable and direct.

        Do not use {// affirmative //} or negative unless the question is a yes or no question.

        ```input
        {#input}
        ```       

        {// The word format is required. don't remove it //}
        Output JSON Format:
        {#print_type(output)}

        JSON:
    "#
}
 
impl<llm, MaybePolishText> v3 {
    client AZURE_GPT4

    prompt #"
        Hi please write whatever the user asks in haiku form.

        {#input.generated_response}
        
       
    "#
} 

// function Thing {
//     input string
//     output string
// }

// impl<llm, Thing> v1 {
//     client AZURE_GPT4
//     prompt #"
//         Generate a story from:
//         ```input
//         {#input}
//         ```

        
//     "#
// } 
 
// function MultiArg {
//     input (convo: ProposedMessage, thing: string)
//     output string
// }

// impl<llm, MultiArg> v1 {
//     client AZURE_GPT4
//     prompt #"
//         Generate a story from:
//         ```input
//         {#input.thing}
//         ```

        
//     "#
// }