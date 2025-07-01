;; Comments
(comment) @comment
(doc_comment) @comment.doc
(block_comment) @comment

;; Operators
[
    "->"
    "|"
    "="
    "?"
    "[]"
    "<"
    ">"
] @operator

;; Delimiters
[
    "{"
    "}"
    "("
    ")"
    ","
    ";"
    ":"
] @punctuation.bracket

;; Strings
[
    (raw_string_literal)
    (quoted_string_literal)
    (string_literal)
    (unquoted_string_literal)
] @string

;; Identifiers
(identifier) @variable

(lambda) @function

(value_expression_keyword) @keyword

;; Types & declarations
(type_expression_block 
  block_keyword: (identifier) @keyword
  name: (identifier) @type.name
  args: (named_argument_list)? @type.arguments
  body: (_) @type.body
)

;; Arguments and parameters
(arguments_list) @parameter
(named_argument_list) @parameter

;; Map keys
(map_key) @property.key

;; Jinja expressions
(jinja_expression) @string

;; Literals
(numeric_literal) @number

;; ── LLM prompt calls ─────────────────────────────────────────────────────────
;;   match only expressions of the form:  prompt  <raw_string_literal>
((value_expression
   name: (identifier) @keyword       ; highlight the word "prompt"
   value: (string_literal) @string)
 (#eq? @keyword "prompt"))          ; but only when the identifier text is "prompt"