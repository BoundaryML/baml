# Type Aliases

Sometimes you may want to use the same complex type in
multiple places.

That might mean mulitple places in your BAML code, or
multiple places in your client SDK code.

In either case, you can improve life by giving that complex
type a "type alias" - a shorter name.

## Recursive type aliases

An advanced use case of type aliases is to define data types
that you couldn't define with ordinary classes. For example,
we show you how you can define a custom `JSON` type! Use
retursive type aliases when you want to build nested data
structures without having to name the intermediate parts.