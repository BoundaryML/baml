; BAML highlight queries.
; Ordered generic-first / specific-last: the tree-sitter highlight crate (and
; nvim-treesitter) give precedence to the pattern that appears LATER in this
; file when several capture the same node.

; ---------------------------------------------------------------------------
; Catch-alls
; ---------------------------------------------------------------------------

(identifier) @variable
(property_identifier) @property

; ---------------------------------------------------------------------------
; Comments
; ---------------------------------------------------------------------------

[
  (comment)
  (block_comment)
] @comment

((comment) @comment.documentation
  (#match? @comment.documentation "^///"))

; ---------------------------------------------------------------------------
; Literals
; ---------------------------------------------------------------------------

(boolean) @boolean
(null) @constant.builtin
(number) @number

[
  (string)
  (byte_string)
] @string

(raw_string) @string
(raw_string_content) @string
(quote) @string
(string_content) @string
(escape_sequence) @string.escape

(literal_type (string) @string)
(literal_type (number) @number)

; ---------------------------------------------------------------------------
; Keywords
; ---------------------------------------------------------------------------

[
  "class"
  "enum"
  "interface"
  "implements"
  "implement"
  "extends"
  "requires"
  "function"
  "client"
  "generator"
  "retry_policy"
  "template_string"
  "test"
  "testset"
  "type_builder"
  "type"
  "let"
  "const"
  "watch"
  "as"
  "with"
  "dynamic"
] @keyword

[
  "if"
  "else"
  "for"
  "while"
  "in"
  "break"
  "continue"
  "return"
  "throw"
  "throws"
  "match"
  "catch"
  "catch_all"
  "spawn"
  "await"
  "defer"
] @keyword.control

[
  "is"
  "instanceof"
] @keyword.operator

; ---------------------------------------------------------------------------
; Operators and punctuation
; ---------------------------------------------------------------------------

[
  "->"
  "=>"
  "="
  "+="
  "-="
  "*="
  "/="
  "%="
  "&="
  "|="
  "^="
  "<<="
  ">>="
  "=="
  "!="
  "<"
  ">"
  "<="
  ">="
  "&&"
  "||"
  "!"
  "&"
  "|"
  "^"
  "~"
  "<<"
  ">>"
  "+"
  "-"
  "*"
  "/"
  "%"
  "??"
  "?."
  "?"
  ".."
  "..."
] @operator

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ";"
  ":"
  "."
] @punctuation.delimiter

(wildcard_pattern) @variable.builtin

; ---------------------------------------------------------------------------
; Types
; ---------------------------------------------------------------------------

(type_path (identifier) @type)
(named_type_argument name: (identifier) @property)
(destructure_pattern type: (identifier) @type)
(generic_parameter name: (identifier) @type)
(client_type kind: (identifier) @type.builtin)

((type_path
   (identifier) @type.builtin)
  (#match? @type.builtin
    "^(string|int|float|bool|null|image|audio|pdf|video|json|void|never|unknown|uint8array|bigint|map)$"))

; ---------------------------------------------------------------------------
; Properties and fields
; ---------------------------------------------------------------------------

(field_declaration name: (identifier) @property)
(config_entry key: (identifier) @property)
(config_entry key: (string) @property)
(prompt_field (property_identifier) @property)
(named_argument name: (identifier) @property)
(field_link interface_field: (identifier) @property)
(field_link class_field: (identifier) @property)
(map_entry key: (string) @property)

; ---------------------------------------------------------------------------
; Parameters and bindings
; ---------------------------------------------------------------------------

(parameter name: (identifier) @variable.parameter)
(binding_pattern name: (identifier) @variable)
(function_type_parameter name: (identifier) @variable.parameter)
(catch_clause binding: (identifier) @variable)
(catch_clause stack_trace: (identifier) @variable)

; A `let x = ...` / `for (let x in ...)` simple binding parses as a bare
; type path in pattern position; color single-segment ones as variables.
(let_declaration
  pattern: (type_pattern (type_path (identifier) @variable)))
(let_declaration
  pattern: (chain_pattern . (type_pattern (type_path (identifier) @variable))))
(for_statement
  pattern: (type_pattern (type_path (identifier) @variable)))

; ---------------------------------------------------------------------------
; Functions and methods
; ---------------------------------------------------------------------------

(call_expression
  function: (identifier) @function)

(call_expression
  function: (member_expression
    property: (property_identifier) @function.method))

(optional_call_expression
  function: (member_expression
    property: (property_identifier) @function.method))

; ---------------------------------------------------------------------------
; Declaration names
; ---------------------------------------------------------------------------

(class_declaration name: (identifier) @type)
(enum_declaration name: (identifier) @type)
(interface_declaration name: (identifier) @type)
(type_alias_declaration name: (identifier) @type)
(client_declaration name: (identifier) @type)
(generator_declaration name: (identifier) @constant)
(retry_policy_declaration name: (identifier) @constant)
(template_string_declaration name: (identifier) @function)
(function_declaration name: (identifier) @function)
(associated_type_declaration name: (identifier) @type)
(enum_variant name: (identifier) @constant)

; ---------------------------------------------------------------------------
; Builtin / special identifiers
; ---------------------------------------------------------------------------

((identifier) @variable.builtin
  (#eq? @variable.builtin "self"))

((identifier) @module
  (#match? @module "^(baml|root|assert|log)$"))

((identifier) @constant.builtin
  (#eq? @constant.builtin "env"))

(member_expression
  object: (identifier) @constant.builtin
  (#eq? @constant.builtin "env")
  property: (property_identifier) @constant)

; Unquoted config values (`openai`, `gpt-4o`, `openai/gpt-4o-mini`)
(config_value_path (identifier) @string.special)

; ---------------------------------------------------------------------------
; Attributes: @description(...), @stream.done, @@dynamic
; ---------------------------------------------------------------------------

(attribute
  "@" @attribute
  name: (attribute_name) @attribute)

(block_attribute
  "@@" @attribute
  name: (attribute_name) @attribute)

; note: `@@dynamic` reuses the keyword token, aliased to identifier in the tree
(attribute_name (identifier) @attribute)

; ---------------------------------------------------------------------------
; Backtick interpolation delimiters and block tags
; ---------------------------------------------------------------------------

(backtick_string
  [
    "`"
    "``"
    "```"
  ] @string)

(interpolation
  "${" @punctuation.special
  "}" @punctuation.special)

[
  (template_else)
  (template_endif)
  (template_endfor)
] @keyword.control

(template_for_open "for" @keyword.control)
(template_if_open "if" @keyword.control)
(template_else_if "else" @keyword.control "if" @keyword.control)
