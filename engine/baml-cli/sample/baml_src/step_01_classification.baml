// BAML is based on the idea of function, and defining create interfaces for
// interacting with models. This helps you with high-quality data collection,
// and making your buisness logic and database never impacted by your
// choice of AI system.

// This example will walk through building a chatbot which helps
// someone interact with our calendar.

// Step 1: Classify the users intent as one of:
// * Book a meeting
// * Ask about our availability
// * Set a reminder

// 1. Define an enum which represents the users intent
enum Intent {
  BookMeeting
  AvailabilityQuery
  SetReminder
}

// Define a function which takes a string and returns any of the intents
function ClassifyIntent {
  input string
  output Intent[]
}

// Actually implement the function, using an llm.
// If you've downloaded BAML extension, you should see
// a "Open Playground" button below this comment, check it out
// to see what this implementation looks like.
impl<llm, ClassifyIntent> v1 {
  // This is the name of the client, which is used to execute this impl.
  // See ./clients.baml file for more details.
  client Main

  // This is the prompt template that is passed to the LLM.
  prompt #"
    Given the question, which of the intents is the user attempting to do?

    Question:
    ```
    {#input}{// you can refer to you input in the prompt here,
    
    also
    yes this is a comment and won't be in the final prompt ;)
    
    //}
    ```

    {#print_enum(Intent)}

    Output format: {#print_type(output)}

    Intent:
  "#

  // Note, even though an LLM returns a string, we parse it into our enum array
  // automatically.
}

{//
To call that code in python, you could do something like this:

```python
import typing
from baml_client import baml
from baml_client.baml_types import Intent

async def my_function(input: str) -> typing.List[Intent]:
  # Note this is all 100% typed, so you get autocomplete and type checking.
  return await baml.ClassifyIntent.get_impl('v1').run(input)
``` 
//}

impl<llm, ClassifyIntent> v2 {
  client Main
  
  // Lets say, we want the LLM to call it "book_a_meeting" instead of "BookMeeting"
  // We can do that by defining a mapping from the enum to the string, but only
  // localize that mapping to this version of the function.
  override Intent {
    BookMeeting @alias(book_a_meeting)
    @description(#"
      When the user wants to book a meeting
    "#)
    AvailabilityQuery @alias(check_availability)
    @description(#"
      When the query is directly asking about availability
    "#)

    // You can even skip an enum value, if you don't want to expose it to the LLM
    SetReminder @skip

    // You can even rename the type of the enum.
    @@alias(User Intents)
  }

  // Notice the prompt is the same as v1, but the implementation is different
  // due to the override above.
  // Even we serialized the enum differently, we still parse the LLM response into the
  // same enum array (Intent[]), meaning you can change the implementation without
  // impacting the rest of your code.
  prompt #"
    Given the question, which of the intents is the user attempting to do?

    Question:
    ```
    {#input}
    ```

    {#print_enum(Intent)}

    Output format: {#print_type(output)}

    Intent:
  "#

}

{//
The python code to call this version of the function is the same as v1, but
uses the v2 impl instead.

```python
import typing
from baml_client import baml
from baml_client.baml_types import Intent

async def my_function(input: str) -> typing.List[Intent]:
  # Note this is all 100% typed, so you get autocomplete and type checking.
  return await baml.ClassifyIntent.get_impl('v2').run(input)

  # In the near future, you'll be able to do:
  # And set which default impl to use (along with A/B testing for routing portions of traffic).
  # return await baml.ClassifyIntent.run(input)
``` 
//}
