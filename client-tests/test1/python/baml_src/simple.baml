function Blah {
  input string
  output string

  default_impl v1
}

impl<llm, Blah> v1 {
  client ResilientGPT4
  prompt #"hello there {#input}"#
} 

impl<llm, Blah> v2 {  
  client ResilientGPT4
  prompt #"whats your name {#input}"#
}

function ExtractVerbs {
  input (title: string, body: string)
  output string[]
}

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4
    api_key env.OPENAI_API_KEY
  }
}



impl<llm, ExtractVerbs> version1 {
  client GPT4
  prompt #"
    Extract the verbs from this paragraph:
    
    Title: {#input.title}
    ---
    {#input.body}
    --
    {// this is a comment inside a prompt! //}
    Return a {#print_type(output)}.

    Response:
  "#
}


enum Intent {
    Refund 
    @alias("k1")
    @description("Customer wants to return a product") 
    
    CancelOrder 
    @alias("k2")
    @description("Customer wants to cancel an order") 

    TechnicalSupport
    @alias("k3")
    @description("Customer needs help with a technical issue unrelated to account creation or login")

    AccountIssue 
    @alias("k4")
    @description("Specifically relates to account-login or account-creation")

    Question 
    @alias("k5")
    @description("Customer has a question")
}


function ClassifyIntent {
  input string
  output Intent
  default_impl version2
}

impl<llm, ClassifyIntent> version1 {
  client GPT4
  prompt #"
    Classify the following INPUT into ONE
    of the following Intents: 

    {#print_enum(Intent)}

    INPUT: {#input}
    
    Response:
  "#
}

impl<llm, ClassifyIntent> version2 {
  client GPT4

  override Intent {
    TechnicalSupport
    @alias("technical-support")
    @description("Customer needs help with a technical issue unrelated to account creation")

    AccountIssue
    @alias("account-issue")
    @description("Specifically relates to account-creation")

    Question
    @skip 
  }

  prompt #"
    Classify the following INPUT into ONE
    of the following Intents: 

    {#print_enum(Intent)}

    INPUT: {#input}
    
    Response:
  "#
}


class IntentWithReasoning {
  reasoning_steps string
  intent Intent
}
 
impl<llm, ClassifyIntent> version3 {
  client GPT4

  // This adapter adds a middleware to the function that makes it output IntentOutputWithCoT instead, and lets us declare some python code to
  // convert the data into the original output -- an Intent. 
  adapter<IntentWithReasoning, output> python#"
    # output is a special variable that contains the output of the LLM that is of type IntentOutputWithCoT. We need to return just the intent to abide by the original function signature.
    return output.intent
  "#

  prompt #"
    Classify the following INPUT into ONE
    of the following Intents: 

    {#print_enum(Intent)}

    INPUT: {#input}
    
    Response JSON:
    {#print_type(output)}
  "#
} 