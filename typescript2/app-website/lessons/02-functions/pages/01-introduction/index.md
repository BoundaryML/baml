# Functions

The key feature of BAML is that LLM prompts are functions.

In most programming languages, functions are defined by mapping
parameters to a return value. In BAML, functions pass arguments
to an LLM (like `GPT4` or `Claude`) through a prompt, then parse
a result from the LLM's response.

Here is an example LLM functinon you could write called `Greet()`.
Greet takes one argument for the user's name, and returns a greeting
as a string.

Click the `Greet > TestGreet` button to try out this function.

You can also experiment with the prompt on line 4 or the test arguments
on line 13.

On the next page, we'll learn more about the different parts of an LLM function.