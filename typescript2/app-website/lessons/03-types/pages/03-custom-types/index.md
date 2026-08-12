# Custom Types

The real power of LLMs in applications comes when you
start defining your own types. BAML allows you to create
your own classes and enums.

Let's create an LLM function that describes characters from a stort story.

The `class` keyword on the right is used to introduce a new
class: `Character`, which collects the properties of a character.

The `enum` keyword introduces a new enum - a mutually exclusive
enumeration. We use it to list the characteristics we are interested
in.

Finally, the `ExtractCharacters` function uses these custom types to
extract characters from an unstructured input (a story).