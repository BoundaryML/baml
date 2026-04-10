# Parts of a Function

Let's examine an LLM function in detail. Our BAML code is
on the right.

An "LLM Function" in BAML has these parts:
  - Function Name (`Greet`)
  - Argument List (`name: string`)
  - Return Type (`string`)
  - Client Name
  - Prompt

## Client Name

The client name is a reference to any of the LLM clients
you have defined. Click the `clients.baml` file to see
how clients are defined. We will cover the definition of
Clients in another section.

## Prompt

This is the main implementation of the LLM function.
The prompt is the primary piece of data sent to the LLM,
and it determines how your function arguments will
be presented to the LLM. We will have more practice with
prompts soon. For now it's enough to note that you can
inject function parameters into your prompts by putting
the parameter's name inside `{{ double_curlys }}`.

We will have a lot more to say about functions and prompting
after we learn a bit about Types!