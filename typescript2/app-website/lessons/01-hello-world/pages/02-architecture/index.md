# BAML Architecture

Here's how BAML works.

First, you define [Types] and [Functions]. The functions you define
will tell an LLM how to turn a set of inputs into an output value.

While creating types and functions, you will be able to run them
in a playground like the one on the right.

All the while, your editor is taking your BAML code and using it to
generate client SDKs in the language of your choice. If you use Python
to develop the rest of your app, we create Python types and Python
functions mirroring the ones you defined in BAML.

Your Python code can call these functions, letting you access the
LLM in a way that is extremely natural from the rest of your app.

Under the hood, when you call the Python (for example) version of
your BAML function, BAML turns your Python values into BAML values,
uses your own prompt to format them for the LLM, call the LLM,
then parse the result into your specified return type, so you
can access it from your Python code.

It's a mouthful, we know. But once you try it, you'll see how
natural it all is. Move on the the next page to see how Functions
look in BAML.