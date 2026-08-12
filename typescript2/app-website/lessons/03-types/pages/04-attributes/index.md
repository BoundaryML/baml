# Attributes

BAML types can be marked with attributes to change the way
they are presented to the LLM or the way they are reflected
in your generated SDK code.

The examples on the left show some of the more common attributes.

- **description:** information about the type that will be
  presented to the LLM within the prompt. Use these to add
  context where the type itself doesn't tell the whole story.
- **alias:** A different name for a field or an enum variant,
  which will be provided to the LLM. Your generated types will
  always use the same names you used in your BAML code. You can
  use `alias` if a different name works better for the LLM.
- **skip:** Skip a field or enum variant when presenting it to
  the LLM. Use this for fields or variants that only your codebase
  cares about.
- **assert:** Enforce that some invariant property holds for
  a type. A value that fails its type's assertion will never
  reach your client code, so use these as LLM guardrails.
- **check:** Similar to assert, verify that the value adheres
  to some property. The check's result will be computed and
  presented to the client as part of the return value, rather
  than producing a hard failure. Use this attribute when you
  want to respond to failures at runtime, for example by
  rendering an error message next to a field with faulty data.