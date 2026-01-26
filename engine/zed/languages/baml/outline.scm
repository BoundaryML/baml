; Type expression blocks (class, enum, etc.)
(type_expression_block
    block_keyword: (identifier) @context
    name: (identifier) @name) @item

; Value expression blocks (function, test, client, client<llm>, retry_policy, generator)
(value_expression_block
    keyword: (value_expression_keyword) @context
    name: (identifier) @name) @item

; Expression functions (fn keyword)
(expr_fn
    name: (identifier) @name) @item

; Template declarations (template_string or string_template keyword)
(template_declaration
    name: (identifier) @name) @item

; Type aliases
(type_alias
    name: (identifier) @name) @item
