---
title: ctx.output_format
---


`{{ ctx.output_format }}` is used within a prompt template (or in any template_string) to print out the function's output schema into the prompt. It describes to the LLM how to generate a structure BAML can parse (usually JSON).

Here's an example of a function with `{{ ctx.output_format }}`, and how it gets rendered by BAML before sending it to the LLM.

**BAML Prompt**

```baml
class Resume {
  name string
  education Education[]
}
function ExtractResume(resume_text: string) -> Resume {
  prompt #"
    Extract this resume:
    ---
    {{ resume_text }}
    ---

    {{ ctx.output_format }}
  "#
}
```

**Rendered prompt**

```text
Extract this resume
---
Aaron V.
Bachelors CS, 2015
UT Austin
---

Answer in JSON using this schema: 
{
  name: string
  education: [
    {
      school: string
      graduation_year: string
    }
  ]
}
```

## Controlling the output_format

`ctx.output_format` can also be called as a function with parameters to customize how the schema is printed, like this:
```text

{{ ctx.output_format(prefix="If you use this schema correctly and I'll tip $400:\n", always_hoist_enums=true)}}
```

Here's the parameters:
<ParamField path="prefix" type="string">
The prefix instruction to use before printing out the schema. 

```text
Answer in this schema correctly I'll tip $400:
{
  ...
}
```
BAML's default prefix varies based on the function's return type.

| Fuction return type | Default Prefix |
| --- | --- |
| Primitive (String) |  |
| Primitive (Int) | `Answer as an ` |
| Primitive (Other) | `Answer as a ` |
| Enum | `Answer with any of the categories:\n` |
| Class | `Answer in JSON using this schema:\n` |
| List | `Answer with a JSON Array using this schema:\n` |
| Union | `Answer in JSON using any of these schemas:\n` |
| Optional | `Answer in JSON using this schema:\n` |

</ParamField>

<ParamField path="always_hoist_enums" type="boolean" > 
Whether to inline the enum definitions in the schema, or print them above. **Default: false**

Note that setting this to `false` means BAML will use heuristics internally to determine
whether or not to hoist. `false` does not mean "never hoist".


**Inlined**
```

Answer in this json schema:
{
  categories: "ONE" | "TWO" | "THREE"
}
```

**hoisted**
```
MyCategory
---
ONE
TWO
THREE

Answer in this json schema:
{
  categories: MyCategory
}
```

<Warning>BAML will always hoist if you add a [description](/docs/snippets/enum#aliases-descriptions) to any of the enum values.</Warning>

</ParamField>

<ParamField path="or_splitter" type="string" >

**Default: ` or `**

If a type is a union like `string | int` or an optional like `string?`, this indicates how it's rendered. 


BAML renders it as `property: string or null` as we have observed some LLMs have trouble identifying what `property: string | null` means (and are better with plain english).

You can always set it to ` | ` or something else for a specific model you use.
</ParamField>

<ParamField path="hoist_classes" type="'auto' | bool | list[string]">

**Default: `"auto"`**

<Info>
Requires BAML Version 0.89+
</Info>

Controls which classes are hoisted in the prompt. Recursive classes are
**always** hoisted because they need to be referenced by name.

Let's use this as an example to visualize the different options:

```baml
class Example {
  a string
  b string
  c NestedClass
  d Node
}

class NestedClass {
  m int
  n int
}

class Node {
  data int
  next Node?
}

function UseExample() -> Example {
  client GPT4
  prompt #"{{ctx.output_format}}"#
}
```

**"auto"**

Only recursive classes are hoisted:

```baml
Node {
  data: int,
  next: Node or null
}

Answer in JSON using this schema:
{
  a: string,
  b: string,
  c: {
    m: int,
    n: int,
  },
  d: Node,
}
```

**false**

Same as `"auto"`.

**true**

Hoist all classes.

```baml
Node {
  data: int,
  next: Node or null
}

Example {
  a: string,
  b: string,
  c: NestedClass,
  d: Node,
}

NestedClass {
  m: int,
  n: int,
}

Answer in JSON using this schema: Example
```

**list[string]**

Hoist only recursive classes and the classes specified in the list. For example
`ctx.output_format(hoist_classes=["NestedClass"])` will hoist `NestedClass`.

```baml
Node {
  data: int,
  next: Node or null
}

NestedClass {
  m: int,
  n: int,
}

Answer in JSON using this schema:
{
  a: string,
  b: string,
  c: NestedClass,
  d: Node,
}
```

</ParamField>

<ParamField path="hoisted_class_prefix" type="string"> 
Prefix of hoisted classes in the prompt. **Default: `<none>`**

This parameter controls the prefix used for hoisted classes as well as the word
used in the render message to refer to the output type, which defaults to
`"schema"`:

```
Answer in JSON using this schema:
```

See examples below.

**Recursive BAML Prompt Example**

```baml
class Node {
  data int
  next Node?
}

class LinkedList {
  head Node?
  len int
}

function BuildLinkedList(input: int[]) -> LinkedList {
  prompt #"
    Build a linked list from the input array of integers.

    INPUT: {{ input }}

    {{ ctx.output_format }}    
  "#
}
```

**Default `hoisted_class_prefix` (none)**

```
Node {
  data: int,
  next: Node or null
}

Answer in JSON using this schema:
{
  head: Node or null,
  len: int
}
```

**Custom Prefix: `hoisted_class_prefix="interface"`**

```
interface Node {
  data: int,
  next: Node or null
}

Answer in JSON using this interface:
{
  head: Node or null,
  len: int
}
```
</ParamField>

## Why BAML doesn't use JSON schema format in prompts
BAML uses "type definitions" or "jsonish" format instead of the long-winded json-schema format.
The tl;dr is that json schemas are
1. 4x more inefficient than "type definitions".
2. very unreadable by humans (and hence models)
3. perform worse than type definitions (especially on deeper nested objects or smaller models)

Read our [full article on json schema vs type definitions](https://www.boundaryml.com/blog/type-definition-prompting-baml)
