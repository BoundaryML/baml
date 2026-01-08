# BAML Parser Tree Reference

Reference-only guide to the BAML Concrete Syntax Tree (CST) produced by the improved parser. Each section pairs a short BAML snippet with the CST emitted by the parser. Only semantic nodes and meaningful tokens are shown; whitespace and comments are omitted unless noted.

## How to Read These Trees

- Keyword tokens use the `_KW` suffix, for example `FUNCTION_KW`.
- Semantic wrappers such as `NAME`, `NAME_REF`, `TYPE_REF`, and `PATH` indicate identifier roles.
- Literal nodes keep delimiters (quotes, hashes, brackets) to demonstrate lossless parsing.
- Error recovery inserts explicit `ERROR` nodes without discarding surrounding tokens.

---

## Keywords

**Source:**

```baml
function class enum client test type retry_policy template_string
```

**CST:**

```text
FUNCTION_KW "function"
CLASS_KW "class"
ENUM_KW "enum"
CLIENT_KW "client"
TEST_KW "test"
TYPE_KW "type"
RETRY_POLICY_KW "retry_policy"
TEMPLATE_STRING_KW "template_string"
```

**Notes:**

- Lexer emits `WORD`; parser remaps by text value.
- Whitespace tokens between keywords are omitted.

---

## Identifiers and Names

### Definition Position (`NAME`)

**Source:**

```baml
function GetUser() {}
```

**CST:**

```text
FUNCTION_DEF
  FUNCTION_KW "function"
  NAME
    IDENT "GetUser"
  PARAMETER_LIST
    L_PAREN "("
    R_PAREN ")"
  LLM_FUNCTION_BODY
    L_BRACE "{"
    R_BRACE "}"
```

**Notes:**

- `NAME` signals a newly defined identifier.

### Reference Position (`NAME_REF`)

**Source:**

```baml
client GPT4 {}
```

**CST:**

```text
CLIENT_DEF
  CLIENT_KW "client"
  NAME
    IDENT "GPT4"
  L_BRACE "{"
  R_BRACE "}"
```

**Notes:**

- The client defines `GPT4`; later references use `NAME_REF`.

### Type Reference (`TYPE_REF` + `PATH`)

**Source:**

```baml
field user: User
```

**CST:**

```text
FIELD
  NAME
    IDENT "user"
  COLON ":"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "User"
```

**Notes:**

- `TYPE_REF` always wraps a `PATH`, even for single identifiers.

---

## String Literals

### Regular String

**Source:**

```baml
"Hello, world!"
```

**CST:**

```text
STRING_LITERAL
  QUOTE "\""
  STRING_CONTENT "Hello, world!"
  QUOTE "\""
```

### Raw String (Single Hash)

**Source:**

```baml
#"Hello, \"quoted\""#
```

**CST:**

```text
RAW_STRING_LITERAL
  HASH "#"
  QUOTE "\""
  STRING_CONTENT "Hello, \"quoted\""
  QUOTE "\""
  HASH "#"
```

### Raw String (Multiple Hashes)

**Source:**

```baml
##"Use #"nested"# hashes"##
```

**CST:**

```text
RAW_STRING_LITERAL
  HASH "#"
  HASH "#"
  QUOTE "\""
  STRING_CONTENT "Use #\"nested\"# hashes"
  QUOTE "\""
  HASH "#"
  HASH "#"
```

### Block String

**Source:**

````baml
```
Line 1
Line 2
```
````

**CST:**

```text
BLOCK_STRING_LITERAL
  BACKTICK_TRIPLE "```"
  STRING_CONTENT "Line 1\nLine 2\n"
  BACKTICK_TRIPLE "```"
```

**Notes:**

- Four-backtick fence wraps the triple-backtick literal.

---

## Type Expressions

### Primitives

**Source:**

```baml
string int bool
```

**CST:**

```text
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "string"
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "int"
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "bool"
```

### Optional Type

**Source:**

```baml
string?
```

**CST:**

```text
OPTIONAL_TYPE
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "string"
  QUESTION "?"
```

### List Type

**Source:**

```baml
List<string>
```

**CST:**

```text
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "List"
  TYPE_ARG_LIST
    LESS "<"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "string"
    GREATER ">"
```

### Map Type

**Source:**

```baml
Map<string, int>
```

**CST:**

```text
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "Map"
  TYPE_ARG_LIST
    LESS "<"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "string"
    COMMA ","
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "int"
    GREATER ">"
```

### Union Type

**Source:**

```baml
Success | Failure | Pending
```

**CST:**

```text
UNION_TYPE
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "Success"
  PIPE "|"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "Failure"
  PIPE "|"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "Pending"
```

### Nested Generics

**Source:**

```baml
List<Map<string, User>>
```

**CST:**

```text
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "List"
  TYPE_ARG_LIST
    LESS "<"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "Map"
      TYPE_ARG_LIST
        LESS "<"
        TYPE_REF
          PATH
            PATH_SEGMENT
              IDENT "string"
        COMMA ","
        TYPE_REF
          PATH
            PATH_SEGMENT
              IDENT "User"
        GREATER ">"
    GREATER ">"
```

---

## Type Alias

**Source:**

```baml
type UserId = string
```

**CST:**

```text
TYPE_ALIAS
  TYPE_KW "type"
  NAME
    IDENT "UserId"
  EQ "="
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "string"
```

---

## Enums

### Basic Enum

**Source:**

```baml
enum Status {
    Success
    Failure
}
```

**CST:**

```text
ENUM_DEF
  ENUM_KW "enum"
  NAME
    IDENT "Status"
  L_BRACE "{"
  ENUM_VARIANT
    NAME
      IDENT "Success"
  ENUM_VARIANT
    NAME
      IDENT "Failure"
  R_BRACE "}"
```

### Enum with Attributes

**Source:**

```baml
enum Priority {
    High @alias("high")
    Low @alias("low")
}
```

**CST:**

```text
ENUM_DEF
  ENUM_KW "enum"
  NAME
    IDENT "Priority"
  L_BRACE "{"
  ENUM_VARIANT
    NAME
      IDENT "High"
    FIELD_ATTRIBUTE
      AT "@"
      NAME_REF
        IDENT "alias"
      ATTRIBUTE_ARGS
        L_PAREN "("
        ATTRIBUTE_ARG
          STRING_LITERAL
            QUOTE "\""
            STRING_CONTENT "high"
            QUOTE "\""
        R_PAREN ")"
  ENUM_VARIANT
    NAME
      IDENT "Low"
    FIELD_ATTRIBUTE
      AT "@"
      NAME_REF
        IDENT "alias"
      ATTRIBUTE_ARGS
        L_PAREN "("
        ATTRIBUTE_ARG
          STRING_LITERAL
            QUOTE "\""
            STRING_CONTENT "low"
            QUOTE "\""
        R_PAREN ")"
  R_BRACE "}"
```

---

## Classes

### Basic Class

**Source:**

```baml
class User {
    name string
    age int
}
```

**CST:**

```text
CLASS_DEF
  CLASS_KW "class"
  NAME
    IDENT "User"
  L_BRACE "{"
  FIELD
    NAME
      IDENT "name"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "string"
  FIELD
    NAME
      IDENT "age"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "int"
  R_BRACE "}"
```

### Optional Field

**Source:**

```baml
class User {
    email string?
}
```

**CST:**

```text
CLASS_DEF
  CLASS_KW "class"
  NAME
    IDENT "User"
  L_BRACE "{"
  FIELD
    NAME
      IDENT "email"
    OPTIONAL_TYPE
      TYPE_REF
        PATH
          PATH_SEGMENT
            IDENT "string"
      QUESTION "?"
  R_BRACE "}"
```

### Field and Block Attributes

**Source:**

```baml
class User {
    id string @unique

    @@description("User model")
}
```

**CST:**

```text
CLASS_DEF
  CLASS_KW "class"
  NAME
    IDENT "User"
  L_BRACE "{"
  FIELD
    NAME
      IDENT "id"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "string"
    FIELD_ATTRIBUTE
      AT "@"
      NAME_REF
        IDENT "unique"
  BLOCK_ATTRIBUTE
    AT "@"
    AT "@"
    NAME_REF
      IDENT "description"
    ATTRIBUTE_ARGS
      L_PAREN "("
      ATTRIBUTE_ARG
        STRING_LITERAL
          QUOTE "\""
          STRING_CONTENT "User model"
          QUOTE "\""
      R_PAREN ")"
  R_BRACE "}"
```

---

## Functions (LLM Style)

### No Parameters

**Source:**

```baml
function GetStatus() -> Status {
    client GPT4
    prompt #"Get status"#
}
```

**CST:**

```text
FUNCTION_DEF
  FUNCTION_KW "function"
  NAME
    IDENT "GetStatus"
  PARAMETER_LIST
    L_PAREN "("
    R_PAREN ")"
  ARROW "->"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "Status"
  LLM_FUNCTION_BODY
    L_BRACE "{"
    CLIENT_FIELD
      CLIENT_KW "client"
      NAME_REF
        IDENT "GPT4"
    PROMPT_FIELD
      PROMPT_KW "prompt"
      RAW_STRING_LITERAL
        HASH "#"
        QUOTE "\""
        STRING_CONTENT "Get status"
        QUOTE "\""
        HASH "#"
    R_BRACE "}"
```

### Single Parameter

**Source:**

```baml
function GetUser(id: int) -> User {
    client GPT4
    prompt #"Get user {{id}}"#
}
```

**CST:**

```text
FUNCTION_DEF
  FUNCTION_KW "function"
  NAME
    IDENT "GetUser"
  PARAMETER_LIST
    L_PAREN "("
    PARAMETER
      NAME
        IDENT "id"
      COLON ":"
      TYPE_REF
        PATH
          PATH_SEGMENT
            IDENT "int"
    R_PAREN ")"
  ARROW "->"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "User"
  LLM_FUNCTION_BODY
    L_BRACE "{"
    CLIENT_FIELD
      CLIENT_KW "client"
      NAME_REF
        IDENT "GPT4"
    PROMPT_FIELD
      PROMPT_KW "prompt"
      RAW_STRING_LITERAL
        HASH "#"
        QUOTE "\""
        STRING_CONTENT "Get user {{id}}"
        QUOTE "\""
        HASH "#"
    R_BRACE "}"
```

### Multiple Parameters

**Source:**

```baml
function Search(query: string, limit: int) -> Results {
    client GPT4
    prompt #"Search"#
}
```

**CST:**

```text
PARAMETER_LIST
  L_PAREN "("
  PARAMETER
    NAME
      IDENT "query"
    COLON ":"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "string"
  COMMA ","
  PARAMETER
    NAME
      IDENT "limit"
    COLON ":"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "int"
  R_PAREN ")"
```

### Complex Return Type

**Source:**

```baml
function GetUsers() -> List<User> {
    client GPT4
    prompt #"Get users"#
}
```

**CST:**

```text
TYPE_REF
  PATH
    PATH_SEGMENT
      IDENT "List"
  TYPE_ARG_LIST
    LESS "<"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "User"
    GREATER ">"
```

---

## Functions (Expression Style — Planned)

> Expression-bodied parsing arrives in Phase 4. The snippet below reflects the intended CST.

**Source:**

```baml
function Double(x: int) -> int {
    return x * 2
}
```

**CST (planned):**

```text
FUNCTION_DEF
  FUNCTION_KW "function"
  NAME
    IDENT "Double"
  PARAMETER_LIST
    PARAMETER
      NAME
        IDENT "x"
      COLON ":"
      TYPE_REF
        PATH
          PATH_SEGMENT
            IDENT "int"
  ARROW "->"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "int"
  EXPR_FUNCTION_BODY
    L_BRACE "{"
    RETURN_STMT
      RETURN_KW "return"
      BINARY_EXPR
        NAME_REF
          IDENT "x"
        STAR "*"
        LITERAL
          INT_LITERAL "2"
    R_BRACE "}"
```

---

## Clients

### Basic Client

**Source:**

```baml
client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4"
    }
}
```

**CST:**

```text
CLIENT_DEF
  CLIENT_KW "client"
  TYPE_ARG_LIST
    LESS "<"
    TYPE_REF
      PATH
        PATH_SEGMENT
          IDENT "llm"
    GREATER ">"
  NAME
    IDENT "GPT4"
  L_BRACE "{"
  CONFIG_BLOCK
    NAME_REF
      IDENT "provider"
    NAME_REF
      IDENT "openai"
  CONFIG_BLOCK
    NAME_REF
      IDENT "options"
    CONFIG_VALUE
      L_BRACE "{"
      CONFIG_ITEM
        NAME_REF
          IDENT "model"
        STRING_LITERAL
          QUOTE "\""
          STRING_CONTENT "gpt-4"
          QUOTE "\""
      R_BRACE "}"
  R_BRACE "}"
```

### Client with Environment Variable

**Source:**

```baml
client<llm> GPT4 {
    provider openai
    options {
        api_key env.OPENAI_API_KEY
    }
}
```

**CST:**

```text
CONFIG_ITEM
  NAME_REF
    IDENT "api_key"
  CONFIG_VALUE
    NAME_REF
      IDENT "env"
    DOT "."
    NAME_REF
      IDENT "OPENAI_API_KEY"
```

---

## Attributes

### Field Attributes

**Source:**

```baml
field email string @unique @description("Email address")
```

**CST:**

```text
FIELD
  NAME
    IDENT "email"
  TYPE_REF
    PATH
      PATH_SEGMENT
        IDENT "string"
  FIELD_ATTRIBUTE
    AT "@"
    NAME_REF
      IDENT "unique"
  FIELD_ATTRIBUTE
    AT "@"
    NAME_REF
      IDENT "description"
    ATTRIBUTE_ARGS
      L_PAREN "("
      ATTRIBUTE_ARG
        STRING_LITERAL
          QUOTE "\""
          STRING_CONTENT "Email address"
          QUOTE "\""
      R_PAREN ")"
```

### Block Attributes

**Source:**

```baml
@@index(["email", "created_at"])
```

**CST:**

```text
BLOCK_ATTRIBUTE
  AT "@"
  AT "@"
  NAME_REF
    IDENT "index"
  ATTRIBUTE_ARGS
    L_PAREN "("
    ATTRIBUTE_ARG
      LIST_LITERAL
        L_BRACKET "["
        STRING_LITERAL
          STRING_CONTENT "email"
        COMMA ","
        STRING_LITERAL
          STRING_CONTENT "created_at"
        R_BRACKET "]"
    R_PAREN ")"
```

---

## Retry Policies

**Source:**

```baml
retry_policy DefaultRetry {
    max_retries 3
    delay_ms 100
}
```

**CST:**

```text
RETRY_POLICY_DEF
  RETRY_POLICY_KW "retry_policy"
  NAME
    IDENT "DefaultRetry"
  L_BRACE "{"
  CONFIG_ITEM
    NAME_REF
      IDENT "max_retries"
    INT_LITERAL "3"
  CONFIG_ITEM
    NAME_REF
      IDENT "delay_ms"
    INT_LITERAL "100"
  R_BRACE "}"
```

---

## Template Strings

**Source:**

```baml
template_string Greeting(name: string) #"
Hello {{name}}
"#
```

**CST:**

```text
TEMPLATE_STRING_DEF
  TEMPLATE_STRING_KW "template_string"
  NAME
    IDENT "Greeting"
  TEMPLATE_PARAM_LIST
    L_PAREN "("
    TEMPLATE_PARAM
      NAME
        IDENT "name"
      COLON ":"
      TYPE_REF
        PATH
          PATH_SEGMENT
            IDENT "string"
    R_PAREN ")"
  RAW_STRING_LITERAL
    HASH "#"
    QUOTE "\""
    STRING_CONTENT "\nHello {{name}}\n"
    QUOTE "\""
    HASH "#"
```

---

## Test Blocks

**Source:**

```baml
test GetUserTest {
    functions [GetUser]
    input { id 1 }
}
```

**CST:**

```text
TEST_DEF
  TEST_KW "test"
  NAME
    IDENT "GetUserTest"
  L_BRACE "{"
  CONFIG_BLOCK
    NAME_REF
      IDENT "functions"
    LIST_LITERAL
      L_BRACKET "["
      NAME_REF
        IDENT "GetUser"
      R_BRACKET "]"
  CONFIG_BLOCK
    NAME_REF
      IDENT "input"
    CONFIG_VALUE
      L_BRACE "{"
      CONFIG_ITEM
        NAME_REF
          IDENT "id"
        INT_LITERAL "1"
      R_BRACE "}"
  R_BRACE "}"
```

---

## Error Recovery

### Missing Closing Brace

**Source:**

```baml
function Incomplete() {
    client GPT4
```

**CST:**

```text
FUNCTION_DEF
  FUNCTION_KW "function"
  NAME
    IDENT "Incomplete"
  PARAMETER_LIST
    L_PAREN "("
    R_PAREN ")"
  LLM_FUNCTION_BODY
    L_BRACE "{"
    CLIENT_FIELD
      CLIENT_KW "client"
      NAME_REF
        IDENT "GPT4"
    ERROR
      // Missing closing brace
```

### Incomplete Parameter List

**Source:**

```baml
function Broken(
```

**CST:**

```text
FUNCTION_DEF
  FUNCTION_KW "function"
  NAME
    IDENT "Broken"
  PARAMETER_LIST
    L_PAREN "("
    ERROR
```

### Missing Field Type

**Source:**

```baml
class C {
    value
}
```

**CST:**

```text
CLASS_DEF
  CLASS_KW "class"
  NAME
    IDENT "C"
  L_BRACE "{"
  FIELD
    NAME
      IDENT "value"
    ERROR
      // Expected type reference
  R_BRACE "}"
```

---

## Complete Example (Abridged)

**Source:**

```baml
class User {
    id string @unique
    email string?
}

enum Status {
    Active
    Inactive
}

client<llm> GPT4 {
    provider openai
}

function GetUser(id: string) -> User {
    client GPT4
    prompt #"Fetch user {{id}}"#
}
```

**CST (abridged):**

```text
FILE
  CLASS_DEF (User)
  ENUM_DEF (Status)
  CLIENT_DEF (GPT4)
  FUNCTION_DEF (GetUser)
```

**Notes:**

- Full tree includes the detailed nodes shown in earlier sections.

---

## Quick Reference

- **Keywords:** `FUNCTION_KW`, `CLASS_KW`, `ENUM_KW`, `CLIENT_KW`, `TEST_KW`, `TYPE_KW`, `RETRY_POLICY_KW`, `TEMPLATE_STRING_KW`, `PROMPT_KW`, `TRUE_KW`, `FALSE_KW`, `NULL_KW`, plus planned expression keywords (`LET_KW`, `RETURN_KW`, `IF_KW`, `ELSE_KW`).
- **Identifier Nodes:** `NAME` (definitions), `NAME_REF` (references), `IDENT`, `TYPE_REF`, `PATH`, `PATH_SEGMENT`.
- **Top-Level Nodes:** `FUNCTION_DEF`, `CLASS_DEF`, `ENUM_DEF`, `CLIENT_DEF`, `TYPE_ALIAS`, `TEST_DEF`, `RETRY_POLICY_DEF`, `TEMPLATE_STRING_DEF`.
- **Supporting Nodes:** `PARAMETER`, `PARAMETER_LIST`, `FIELD`, `ENUM_VARIANT`, `BLOCK_ATTRIBUTE`, `FIELD_ATTRIBUTE`, `CONFIG_BLOCK`, `CONFIG_ITEM`, `CONFIG_VALUE`, `TYPE_ARG_LIST`, `OPTIONAL_TYPE`, `UNION_TYPE`.
- **Diagnostics:** `ERROR` nodes preserve location information during recovery.

Keep this document as the canonical reference when evolving the parser, updating snapshots, or building semantic analyses.
