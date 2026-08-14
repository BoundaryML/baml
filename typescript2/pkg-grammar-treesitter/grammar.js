/**
 * Tree-sitter grammar for BAML (Boundary Markup Language).
 *
 * Source of truth for the language shape:
 *   - baml_language/crates/baml_compiler_lexer/src/tokens.rs   (token forms)
 *   - baml_language/crates/baml_compiler_syntax/src/syntax_kind.rs (node taxonomy)
 *   - baml_language/crates/baml_compiler_parser/src/parser.rs  (precedence table)
 *   - typescript2/pkg-grammar/tests/fixtures/*.baml            (the quality bar)
 *
 * Design notes:
 *   - No external scanner. Raw strings (`#"..."#` .. `####"..."####`) are
 *     handled with per-level delimiter tokens plus a high-lexical-precedence
 *     content token; the closing delimiter always wins by longest-match, which
 *     exactly mirrors the real parser's "N hashes close N hashes" rule.
 *   - Statements inside blocks may omit `;` (the real parser is newline
 *     tolerant), so `;` is modeled as an optional terminator / empty statement.
 *   - Identifiers may contain hyphens (`gpt-4o`) and `$`-joined segments
 *     (`Foo$bar`), mirroring the real lexer's Word regex. `a-b` is therefore a
 *     single identifier, exactly like the real lexer (subtraction requires
 *     whitespace or a non-word operand, which is what the fixtures do).
 *
 * Binary precedence mirrors parser.rs::infix_binding_power (higher = tighter).
 */

const PREC = {
  ASSIGN: -1, // = += -= ... (right assoc; parser bp (2,1))
  CATCH: 1, //   expr catch (e) { ... }  — binds looser than any operator
  COALESCE: 2, // ?? (parser forbids mixing with ||; we just rank it low)
  OR: 3, //      ||
  AND: 4, //     &&
  BIT_OR: 5, //  |
  BIT_XOR: 6, // ^
  BIT_AND: 7, // &
  EQUALITY: 8, //  == !=
  COMPARE: 9, //   < > <= >= instanceof is
  SHIFT: 10, //    << >>
  ADD: 11, //      + -
  MUL: 12, //      * / %
  UNARY: 13, //    ! - ~ await
  CALL: 14, //     f(x)  x[i]
  MEMBER: 15, //   a.b  a?.b  a.as<T>
};

// Matches the lexer's Word token: hyphens allowed inside, `$`-joined segments,
// or a `$`-prefixed word ($watch). A trailing `$` is rejected (see tokens.rs).
const IDENT_REGEX =
  /(\$[a-zA-Z_][a-zA-Z0-9_]*)|([a-zA-Z_][a-zA-Z0-9_-]*(\$[a-zA-Z_][a-zA-Z0-9_-]*)*)/;

function commaSep1(rule) {
  return seq(rule, repeat(seq(',', rule)), optional(','));
}

function commaSep(rule) {
  return optional(commaSep1(rule));
}

// Raw string bodies: everything that is not a quote, as one high-precedence
// token so it beats comments/whitespace extras inside raw string content.
// A lone `"` inside the body is its own token; the close delimiter (`"#`,
// `"##`, ...) wins over it by longest-match. See tokens.rs raw string docs.
const RAW_CHUNK = token(prec(2, /[^"]+/));

// Backtick string bodies: text up to a backtick, backslash, or dollar.
const BACKTICK_CHUNK = token(prec(2, /[^`\\$]+/));

// Raw-string delimiter depth. Matches the TextMate grammar's
// MAX_DELIMITER = 8 (pkg-grammar/src/baml.ts): the lexer allows any hash
// count, but 8 covers every realistic string.
const MAX_RAW_STRING_HASHES = 8;

function rawString($, hashes) {
  return seq(
    '#'.repeat(hashes) + '"',
    optional($.raw_string_content),
    '"' + '#'.repeat(hashes),
  );
}

function backtickString($, ticks) {
  const tick = '`'.repeat(ticks);
  // Inside an N-tick string, runs of fewer than N backticks are content.
  const innerTicks = [];
  for (let i = 1; i < ticks; i++) innerTicks.push('`'.repeat(i));
  return seq(
    tick,
    repeat(
      choice(
        alias(BACKTICK_CHUNK, $.string_content),
        ...innerTicks.map((t) => alias(t, $.string_content)),
        $.escape_sequence,
        $.interpolation,
        alias('$', $.string_content),
      ),
    ),
    tick,
  );
}

module.exports = grammar({
  name: 'baml',

  word: ($) => $.identifier,

  extras: ($) => [/\s/, $.comment, $.block_comment],

  supertypes: ($) => [$._expression, $._type, $._pattern, $._statement],

  conflicts: ($) => [
    // `Foo {` — constructor body vs. the `{` of a following block (then-block
    // of `if`, match block, test body, ...). GLR forks; the constructor wins
    // via dynamic precedence only when both parses complete.
    [$.constructor_expression, $._expression],
    // `(x` — lambda parameter list vs. parenthesized expression.
    [$.parameter, $._expression],
    [$.function_type_parameters, $.lambda_parameters],
    [$.function_type_parameter, $.parameter],
    [$.parameter, $.type_path],
    [$.parameter, $.type_path, $._expression],
    // `(int` in type position — function-type params vs parenthesized type.
    [$.function_type_parameter, $.parenthesized_type],
    [$.function_type_parameter, $.parenthesized_type, $.type_pattern],
    // `f <` — explicit call type arguments vs. less-than comparison.
    [$.call_expression, $.binary_expression, $.unary_expression],
    [$.call_expression, $.binary_expression],
    [$.call_expression, $.binary_expression, $.await_expression],
    // Pattern space: `Foo` may open a destructure, a type path, or a plain
    // binding-ish name; `(` may open a paren pattern or a parenthesized type.
    [$.binding_pattern, $.destructure_pattern],
    // `let x` in for-init vs pattern chain; `x : t` chain vs field pattern.
    // Test bodies: `args { ... }` parses both as a config entry and as two
    // juxtaposed expression statements; config entry wins by dynamic prec.
    [$.config_entry, $._expression],
    [$.config_entry, $._expression, $.constructor_expression],
    [$.config_block, $.constructor_body],
    // `f(name = value)` named argument vs assignment expression.
    [$._expression, $.named_argument],
    [$._expression, $.named_type_argument],
    [$.type_path, $._expression],
    [$.type_path, $._expression, $.constructor_expression],
    [$.literal_type, $._expression],
    // `v is int | bool` — the union belongs to the pattern, not to a bitwise
    // `|` on the is-expression result (dynamic prec on union_pattern).
    [$._is_pattern, $.is_union_pattern],
    [$.config_value_path, $._expression],
    [$._config_value, $._expression],
    [$.config_block, $.block],
    [$.config_block, $._statement],
    [$.config_array, $.array_expression],
    // `const` at statement start: declaration vs `const`-as-identifier
    // (the real lexer has no `const` keyword; it is contextual).
    [$.let_declaration, $.const_identifier],
    [$.while_statement, $.const_identifier],
    [$.if_expression, $.const_identifier],
    [$._for_in_header, $.template_for_open],
  ],

  rules: {
    // ==========================================================================
    // Top level
    // ==========================================================================

    source_file: ($) => repeat($._declaration),

    _declaration: ($) =>
      choice(
        $.class_declaration,
        $.enum_declaration,
        $.interface_declaration,
        $.implements_for_declaration,
        $.function_declaration,
        $.client_declaration,
        $.generator_declaration,
        $.retry_policy_declaration,
        $.template_string_declaration,
        $.type_alias_declaration,
        $.test_declaration,
        $.testset_declaration,
        $.let_declaration,
        $.empty_statement,
      ),

    // ==========================================================================
    // Class
    // ==========================================================================

    class_declaration: ($) =>
      seq(
        'class',
        field('name', $.identifier),
        optional(field('type_parameters', $.generic_parameters)),
        field('body', $.class_body),
      ),

    class_body: ($) =>
      seq('{', repeat($._class_member), '}'),

    _class_member: ($) =>
      choice(
        $.field_declaration,
        $.function_declaration,
        $.implements_block,
        $.block_attribute,
        $.empty_statement,
      ),

    // `name string`, `name: string`, trailing `,`/`;`, attributes possibly on
    // following lines (the parser is newline-blind here; so are we).
    field_declaration: ($) =>
      prec.right(
        seq(
          field('name', $.identifier),
          optional(':'),
          field('type', $._type),
          repeat($.attribute),
          optional(choice(',', ';')),
        ),
      ),

    // ==========================================================================
    // Enum
    // ==========================================================================

    enum_declaration: ($) =>
      seq('enum', field('name', $.identifier), field('body', $.enum_body)),

    enum_body: ($) =>
      seq(
        '{',
        repeat(choice($.enum_variant, $.block_attribute, $.empty_statement)),
        '}',
      ),

    enum_variant: ($) =>
      prec.right(
        seq(
          field('name', $.identifier),
          repeat($.attribute),
          optional(choice(',', ';')),
        ),
      ),

    // ==========================================================================
    // Interface / implements
    // ==========================================================================

    interface_declaration: ($) =>
      seq(
        'interface',
        field('name', $.identifier),
        optional(field('type_parameters', $.generic_parameters)),
        optional($.requires_clause),
        field('body', $.interface_body),
      ),

    requires_clause: ($) => seq('requires', commaSep1($._type)),

    interface_body: ($) =>
      seq(
        '{',
        repeat(
          choice(
            $.field_declaration,
            $.function_declaration,
            $.associated_type_declaration,
            $.block_attribute,
            $.empty_statement,
          ),
        ),
        '}',
      ),

    // `type Item extends Bound = Default` in interfaces / implements blocks.
    associated_type_declaration: ($) =>
      prec.right(
        seq(
          'type',
          field('name', $.identifier),
          optional(seq('extends', field('bound', $._type))),
          optional(seq('=', field('value', $._type))),
          optional(choice(',', ';')),
        ),
      ),

    // `implements Iface<Args> { ... }` inside a class body. The lexer has
    // both `implements` and `implement` (tokens.rs), and the parser accepts
    // them interchangeably here and in `implements … for` (parser.rs
    // parse_implements_block / parse_implements_for).
    implements_block: ($) =>
      seq(
        choice('implements', 'implement'),
        field('interface', $._type),
        field('body', $.implements_body),
      ),

    // Top-level `implements<T> Iface<T> for Target<T> { ... }`.
    implements_for_declaration: ($) =>
      seq(
        choice('implements', 'implement'),
        optional(field('type_parameters', $.generic_parameters)),
        field('interface', $._type),
        'for',
        field('target', $._type),
        field('body', $.implements_body),
      ),

    implements_body: ($) =>
      seq(
        '{',
        repeat(
          choice(
            $.function_declaration,
            $.associated_type_declaration,
            $.field_link,
            $.empty_statement,
          ),
        ),
        '}',
      ),

    // `interface_field as class_field`
    field_link: ($) =>
      prec.right(
        seq(
          field('interface_field', $.identifier),
          'as',
          field('class_field', $.identifier),
          optional(choice(',', ';')),
        ),
      ),

    // ==========================================================================
    // Function
    // ==========================================================================

    // Also used for interface method signatures (no body) and class methods.
    function_declaration: ($) =>
      prec.right(
        seq(
          'function',
          field('name', $.identifier),
          optional(field('type_parameters', $.generic_parameters)),
          field('parameters', $.parameter_list),
          optional(seq('->', field('return_type', $._type))),
          optional($.throws_clause),
          optional(field('body', $.function_body)),
        ),
      ),

    parameter_list: ($) => seq('(', commaSep($.parameter), ')'),

    parameter: ($) =>
      seq(
        field('name', $.identifier),
        optional(seq(':', field('type', $._type))),
        optional(seq('=', field('default', $._expression))),
      ),

    throws_clause: ($) => seq('throws', field('error', $._type)),

    // A function body is either an LLM body (client/prompt fields) or an
    // expression body (statements). Both can also mix with header comments.
    function_body: ($) =>
      seq(
        '{',
        repeat(choice($.client_field, $.prompt_field, $._statement)),
        '}',
      ),

    // `client GPT4` | `client: Brain,` | `client "openai/gpt-4o"`
    // | `client openai/gpt-4o-mini`
    client_field: ($) =>
      seq(
        'client',
        optional(':'),
        field('value', choice($.string, $.config_value_path)),
        optional(','),
      ),

    // `prompt #"..."#` | `prompt: #"..."#,`
    prompt_field: ($) =>
      seq(
        alias('prompt', $.property_identifier),
        optional(':'),
        field('value', $.raw_string),
        optional(','),
      ),

    // ==========================================================================
    // client / generator / retry_policy (config blocks)
    // ==========================================================================

    client_declaration: ($) =>
      seq(
        'client',
        optional($.client_type),
        field('name', $.identifier),
        field('body', $.config_block),
      ),

    client_type: ($) => seq('<', field('kind', $.identifier), '>'),

    generator_declaration: ($) =>
      seq('generator', field('name', $.identifier), field('body', $.config_block)),

    retry_policy_declaration: ($) =>
      seq('retry_policy', field('name', $.identifier), field('body', $.config_block)),

    config_block: ($) =>
      seq('{', repeat(choice($.config_entry, $.empty_statement)), '}'),

    // `provider openai` | `provider: anthropic,` | `"string key" "value"`
    // Dynamic precedence: inside test bodies a config entry like
    // `args { ... }` also parses as juxtaposed expression statements; the
    // config interpretation is the semantically correct one.
    config_entry: ($) =>
      prec.dynamic(
        2,
        prec.right(
          seq(
            field('key', choice($.identifier, $.string, alias('retry_policy', $.identifier))),
            optional(':'),
            field('value', $._config_value),
            optional(choice(',', ';')),
          ),
        ),
      ),

    _config_value: ($) =>
      choice(
        $.string,
        $.raw_string,
        $.number,
        $.boolean,
        $.null,
        $.config_block,
        $.config_array,
        $.config_value_path,
      ),

    config_array: ($) =>
      seq('[', repeat(seq($._config_value, optional(','))), ']'),

    // Unquoted config values: `openai`, `gpt-4o`, `env.OPENAI_API_KEY`,
    // `openai/gpt-4o-mini`. Slash segments are lexed immediately so that a
    // following `//` comment still wins by longest match.
    config_value_path: ($) =>
      prec.right(
        seq(
          $.identifier,
          repeat(
            choice(
              seq('.', $.identifier),
              seq('/', alias(token.immediate(/[a-zA-Z0-9_.-]+/), $.identifier)),
            ),
          ),
        ),
      ),

    // ==========================================================================
    // template_string
    // ==========================================================================

    template_string_declaration: ($) =>
      seq(
        'template_string',
        field('name', $.identifier),
        optional(field('parameters', $.parameter_list)),
        field('body', $.raw_string),
      ),

    // ==========================================================================
    // test / testset / type_builder
    // ==========================================================================

    test_declaration: ($) =>
      seq(
        'test',
        optional(field('name', $._test_name)),
        field('body', $.test_body),
      ),

    // Test/testset names: `"literal"`, `ident`, or a dynamic concatenation
    // like `"prefix" + baml.unstable.string(i)` (full expressions would be
    // ambiguous with the body block).
    _test_name: ($) =>
      choice(
        $.string,
        $.identifier,
        alias($.test_name_concat, $.binary_expression),
      ),

    test_name_concat: ($) =>
      seq(
        field('left', choice($.string, $.identifier)),
        field('operator', '+'),
        field('right', $._expression),
      ),

    test_body: ($) =>
      seq(
        '{',
        repeat(
          choice(
            $.type_builder_block,
            $.config_entry,
            $._statement,
          ),
        ),
        '}',
      ),

    testset_declaration: ($) =>
      seq(
        'testset',
        optional(field('name', $._test_name)),
        field('body', $.test_body),
      ),

    type_builder_block: ($) =>
      seq(
        'type_builder',
        '{',
        repeat(
          choice(
            $.class_declaration,
            $.enum_declaration,
            $.dynamic_type_declaration,
            $.type_alias_declaration,
          ),
        ),
        '}',
      ),

    // `dynamic class Resume { ... }` / `dynamic enum Job { ... }`
    dynamic_type_declaration: ($) =>
      seq('dynamic', choice($.class_declaration, $.enum_declaration)),

    // ==========================================================================
    // type alias
    // ==========================================================================

    type_alias_declaration: ($) =>
      prec.right(
        seq(
          'type',
          field('name', $.identifier),
          optional(field('type_parameters', $.generic_parameters)),
          '=',
          field('value', $._type),
          optional(';'),
        ),
      ),

    // ==========================================================================
    // Generic parameters (declaration site)
    // ==========================================================================

    generic_parameters: ($) => seq('<', commaSep1($.generic_parameter), '>'),

    generic_parameter: ($) =>
      seq(
        field('name', $.identifier),
        optional(seq('extends', field('bound', seq($._type, repeat(seq('&', $._type)))))),
      ),

    // ==========================================================================
    // Attributes
    // ==========================================================================

    attribute: ($) =>
      seq(
        '@',
        field('name', $.attribute_name),
        optional(field('arguments', $.attribute_arguments)),
      ),

    block_attribute: ($) =>
      seq(
        '@@',
        field('name', $.attribute_name),
        optional(field('arguments', $.attribute_arguments)),
      ),

    // `description`, `stream.done`, `dynamic` (keyword reused as a name).
    attribute_name: ($) =>
      seq(
        choice($.identifier, alias('dynamic', $.identifier)),
        repeat(seq('.', $.identifier)),
      ),

    attribute_arguments: ($) => seq('(', commaSep($._expression), ')'),

    // ==========================================================================
    // Types
    // ==========================================================================

    _type: ($) =>
      choice(
        $.union_type,
        $.optional_type,
        $.array_type,
        $.type_path,
        $.function_type,
        $.parenthesized_type,
        $.literal_type,
      ),

    union_type: ($) =>
      prec.left(1, seq(field('left', $._type), '|', field('right', $._type))),

    // Postfix suffixes bind tighter than `|`: `int | string[]` is
    // `int | (string[])`. Optional `T?` and array `T[]` may stack.
    optional_type: ($) => prec(3, seq(field('type', $._type), '?')),

    array_type: ($) => prec(3, seq(field('type', $._type), '[', ']')),

    // `string`, `User`, `baml.iter.Iterator<Item = string, Error = never>`,
    // `map<string, int>`, `U.CompareError`
    type_path: ($) =>
      prec.right(
        seq(
          $.identifier,
          repeat(seq('.', $.identifier)),
          optional(field('type_arguments', $.type_arguments)),
        ),
      ),

    type_arguments: ($) =>
      seq('<', commaSep1(choice($._type, $.named_type_argument)), '>'),

    named_type_argument: ($) =>
      seq(field('name', $.identifier), '=', field('type', $._type)),

    // `(x: int, y: int) -> string throws E` — the return type extends as far
    // right as possible (`(int) -> T | (string) -> T` nests into the return).
    function_type: ($) =>
      prec.right(
        2,
        seq(
          field('parameters', $.function_type_parameters),
          '->',
          field('return_type', $._type),
          optional($.throws_clause),
        ),
      ),

    function_type_parameters: ($) =>
      seq('(', commaSep($.function_type_parameter), ')'),

    function_type_parameter: ($) =>
      choice(
        seq(field('name', $.identifier), ':', field('type', $._type)),
        field('type', $._type),
      ),

    parenthesized_type: ($) => seq('(', $._type, ')'),

    // Literal types: `"user" | "assistant"`, `2 | 3`, `true | false`
    literal_type: ($) => choice($.string, $.number, $.boolean),

    // ==========================================================================
    // Statements
    // ==========================================================================

    _statement: ($) =>
      choice(
        $.let_declaration,
        $.expression_statement,
        $.while_statement,
        $.for_statement,
        $.defer_statement,
        $.test_declaration,
        $.testset_declaration,
        $.function_declaration,
        $.empty_statement,
      ),

    empty_statement: (_) => ';',

    expression_statement: ($) => prec.right(seq($._expression, optional(';'))),

    // `let x = v;` `const y: int = x;` `watch let z = v;`
    // `const User { name } = user;` `let v: int = value else { return 0; };`
    let_declaration: ($) =>
      prec.right(
        prec.dynamic(
          1,
          seq(
          optional('watch'),
          choice('let', 'const'),
          field('pattern', $._pattern),
            optional(seq('=', field('value', $._expression))),
            optional(field('else', $.else_clause)),
            optional(';'),
          ),
        ),
      ),

    else_clause: ($) => seq('else', field('body', $.block)),

    while_statement: ($) =>
      prec.right(
        seq(
          'while',
          choice(
            field('condition', $._expression),
            seq(
              choice('let', 'const'),
              field('pattern', $._pattern),
              '=',
              field('value', $._expression),
            ),
          ),
          field('body', $.block),
          optional(';'),
        ),
      ),

    for_statement: ($) =>
      prec.right(
        seq(
          'for',
          choice(
            seq('(', $._for_header, ')'),
            $._for_in_header,
          ),
          field('body', $.block),
          optional(';'),
        ),
      ),

    _for_header: ($) => choice($._for_in_header, $._for_c_header),

    _for_in_header: ($) =>
      seq(
        choice('let', 'const'),
        field('pattern', $._pattern),
        'in',
        field('iterable', $._expression),
      ),

    _for_c_header: ($) =>
      seq(
        field('initializer', $.let_declaration),
        // let_declaration consumes its own `;`; two more sections follow.
        field('condition', $._expression),
        ';',
        field('update', $._expression),
      ),

    defer_statement: ($) =>
      prec.right(seq('defer', field('body', $.block), optional(';'))),

    // ==========================================================================
    // Expressions
    // ==========================================================================

    _expression: ($) =>
      choice(
        $.identifier,
        $.number,
        $.boolean,
        $.null,
        $.string,
        $.byte_string,
        $.raw_string,
        $.backtick_string,
        $.member_expression,
        $.upcast_expression,
        $.index_expression,
        $.call_expression,
        $.optional_call_expression,
        $.constructor_expression,
        $.parenthesized_expression,
        $.array_expression,
        $.map_expression,
        $.lambda_expression,
        $.block,
        $.if_expression,
        $.match_expression,
        $.catch_expression,
        $.is_expression,
        $.binary_expression,
        $.coalesce_expression,
        $.unary_expression,
        $.assignment_expression,
        $.await_expression,
        $.spawn_expression,
        $.throw_expression,
        $.return_expression,
        $.break_expression,
        $.continue_expression,
        alias($.const_identifier, $.identifier),
      ),

    // The real lexer has no `const` keyword; `let const = x; const` is legal.
    const_identifier: (_) => prec.dynamic(-1, 'const'),

    member_expression: ($) =>
      prec(
        PREC.MEMBER,
        seq(
          field('object', $._expression),
          field('operator', choice('.', '?.')),
          field('property', alias($.identifier, $.property_identifier)),
        ),
      ),

    // `expr.as<Iface>` — explicit interface upcast (BEP-044).
    upcast_expression: ($) =>
      prec(
        PREC.MEMBER,
        seq(
          field('object', $._expression),
          '.',
          'as',
          '<',
          field('type', $._type),
          '>',
        ),
      ),

    index_expression: ($) =>
      prec(
        PREC.CALL,
        seq(
          field('object', $._expression),
          optional(field('operator', '?.')),
          '[',
          field('index', $._expression),
          ']',
        ),
      ),

    call_expression: ($) =>
      prec(
        PREC.CALL,
        seq(
          field('function', $._expression),
          optional(field('type_arguments', $.type_arguments)),
          field('arguments', $.arguments),
        ),
      ),

    // `callback?.(42)`
    optional_call_expression: ($) =>
      prec(
        PREC.CALL,
        seq(
          field('function', $._expression),
          '?.',
          field('arguments', $.arguments),
        ),
      ),

    arguments: ($) => seq('(', commaSep(choice($._expression, $.named_argument)), ')'),

    // `baml.spawn.options(cancel = tok)`
    named_argument: ($) =>
      seq(field('name', $.identifier), '=', field('value', $._expression)),

    // `User { name: "x", ...rest }` — fields require `:` (or spread), which
    // is what lets GLR discard bogus constructor parses of `if x {` blocks.
    constructor_expression: ($) =>
      prec.dynamic(
        1,
        seq(
          field('type', choice($.identifier, $.member_expression)),
          optional(field('type_arguments', $.type_arguments)),
          field('body', $.constructor_body),
        ),
      ),

    constructor_body: ($) =>
      seq('{', commaSep(choice($.constructor_field, $.spread_element)), '}'),

    constructor_field: ($) =>
      seq(
        field('name', alias($.identifier, $.property_identifier)),
        ':',
        field('value', $._expression),
      ),

    spread_element: ($) => seq('...', $._expression),

    parenthesized_expression: ($) => seq('(', $._expression, ')'),

    array_expression: ($) =>
      seq('[', commaSep(choice($._expression, $.spread_element)), ']'),

    // `{ "a": 1, "b": [2] }` — JSON-ish map literal (keys are strings).
    map_expression: ($) =>
      seq('{', commaSep1($.map_entry), '}'),

    map_entry: ($) =>
      seq(field('key', $.string), ':', field('value', $._expression)),

    // `(x: int) -> int { x * 2 }` | `(x) -> { ... }` | `() -> int { x }`
    lambda_expression: ($) =>
      seq(
        field('parameters', $.lambda_parameters),
        '->',
        optional(field('return_type', $._type)),
        optional($.throws_clause),
        field('body', $.block),
      ),

    lambda_parameters: ($) => seq('(', commaSep($.parameter), ')'),

    block: ($) => seq('{', repeat($._statement), '}'),

    // `if (cond) {..} else ..` | `if cond {..}` | `if let p = e {..}` |
    // `if const p: T = e {..}`
    if_expression: ($) =>
      prec.right(
        seq(
          'if',
          choice(
            field('condition', $._expression),
            seq(
              choice('let', 'const'),
              field('pattern', $._pattern),
              '=',
              field('value', $._expression),
            ),
          ),
          field('consequence', $.block),
          optional(seq('else', field('alternative', choice($.block, $.if_expression)))),
        ),
      ),

    match_expression: ($) =>
      seq(
        'match',
        field('value', $._expression),
        field('body', $.match_body),
      ),

    match_body: ($) => seq('{', repeat($.match_arm), '}'),

    match_arm: ($) =>
      prec.right(
        seq(
          field('pattern', $._pattern),
          optional(field('guard', $.match_guard)),
          '=>',
          field('value', $._expression),
          optional(choice(',', ';')),
        ),
      ),

    match_guard: ($) => seq('if', $._expression),

    // `expr catch (e) { arms } catch_all (e2) { arms }` — postfix, chains left.
    catch_expression: ($) =>
      prec.left(PREC.CATCH, seq($._expression, $.catch_clause)),

    catch_clause: ($) =>
      seq(
        choice('catch', 'catch_all'),
        optional(
          seq(
            '(',
            field('binding', $.identifier),
            optional(seq(',', field('stack_trace', $.identifier))),
            ')',
          ),
        ),
        '{',
        repeat($.match_arm),
        '}',
      ),

    // `v is int | bool` — Rust matches!-style pattern test. Like the real
    // parser's condition-position rule, the RHS suppresses top-level
    // destructure patterns so `if r is Empty { ... }` keeps the `{` for the
    // then-block (write `(r is Empty { f })` to destructure).
    is_expression: ($) =>
      prec.left(
        PREC.COMPARE,
        seq(field('value', $._expression), 'is', field('pattern', $._is_pattern)),
      ),

    _is_pattern: ($) =>
      choice($._is_pattern_atom, alias($.is_union_pattern, $.union_pattern)),

    is_union_pattern: ($) =>
      prec.right(
        prec.dynamic(
          1,
          seq($._is_pattern_atom, repeat1(seq('|', $._is_pattern_atom))),
        ),
      ),

    _is_pattern_atom: ($) =>
      choice(
        $.binding_pattern,
        $.wildcard_pattern,
        $.array_pattern,
        $.paren_pattern,
        $.type_pattern,
      ),

    binary_expression: ($) => {
      const table = [
        ['||', PREC.OR],
        ['&&', PREC.AND],
        ['|', PREC.BIT_OR],
        ['^', PREC.BIT_XOR],
        ['&', PREC.BIT_AND],
        ['==', PREC.EQUALITY],
        ['!=', PREC.EQUALITY],
        ['<', PREC.COMPARE],
        ['>', PREC.COMPARE],
        ['<=', PREC.COMPARE],
        ['>=', PREC.COMPARE],
        ['instanceof', PREC.COMPARE],
        ['<<', PREC.SHIFT],
        ['>>', PREC.SHIFT],
        ['+', PREC.ADD],
        ['-', PREC.ADD],
        ['*', PREC.MUL],
        ['/', PREC.MUL],
        ['%', PREC.MUL],
      ];
      return choice(
        ...table.map(([op, p]) =>
          prec.left(
            p,
            seq(
              field('left', $._expression),
              field('operator', op),
              field('right', $._expression),
            ),
          ),
        ),
      );
    },

    // `a ?? b` — distinct node (the parser assembles it from two `?` tokens).
    coalesce_expression: ($) =>
      prec.left(
        PREC.COALESCE,
        seq(field('left', $._expression), '??', field('right', $._expression)),
      ),

    unary_expression: ($) =>
      prec(
        PREC.UNARY,
        seq(field('operator', choice('!', '-', '~', '+')), field('operand', $._expression)),
      ),

    assignment_expression: ($) =>
      prec.right(
        PREC.ASSIGN,
        seq(
          field('left', $._expression),
          field(
            'operator',
            choice('=', '+=', '-=', '*=', '/=', '%=', '&=', '|=', '^=', '<<=', '>>='),
          ),
          field('right', $._expression),
        ),
      ),

    await_expression: ($) =>
      prec(PREC.UNARY, seq('await', field('value', $._expression))),

    // `spawn { ... }` | `spawn with baml.spawn.options(...) { ... }`
    spawn_expression: ($) =>
      seq(
        'spawn',
        optional(seq('with', field('options', $._expression))),
        field('body', $.block),
      ),

    throw_expression: ($) => prec.right(seq('throw', field('value', $._expression))),

    return_expression: ($) =>
      prec.right(seq('return', optional(field('value', $._expression)))),

    break_expression: (_) => prec.right('break'),

    continue_expression: (_) => prec.right('continue'),

    // ==========================================================================
    // Patterns (let / match arms / catch arms / is)
    //   PATTERN := UNION (':' UNION)*     (chain narrows)
    //   UNION   := ATOM ('|' ATOM)*
    //   ATOM    := 'let' name | '_' | destructure | array | '(' PATTERN ')' | type
    // ==========================================================================

    _pattern: ($) => choice($.chain_pattern, $._pattern_union),

    chain_pattern: ($) =>
      seq($._pattern_union, repeat1(seq(':', $._pattern_union))),

    _pattern_union: ($) => choice($.union_pattern, $._pattern_atom),

    union_pattern: ($) =>
      prec.dynamic(1, seq($._pattern_atom, repeat1(seq('|', $._pattern_atom)))),

    _pattern_atom: ($) =>
      choice(
        $.binding_pattern,
        $.wildcard_pattern,
        $.destructure_pattern,
        $.array_pattern,
        $.paren_pattern,
        $.type_pattern,
      ),

    // `let x` (also the leading name of `let x: int = ...` via chain).
    binding_pattern: ($) => seq('let', field('name', choice($.identifier, '_'))),

    wildcard_pattern: (_) => '_',

    // `PatternRecord { id, label: let l }` | `let Envelope { record }` |
    // `name.space.Type { field }`
    destructure_pattern: ($) =>
      seq(
        optional('let'),
        field('type', seq($.identifier, repeat(seq('.', $.identifier)))),
        '{',
        commaSep($.field_pattern),
        '}',
      ),

    field_pattern: ($) =>
      seq(
        field('name', alias($.identifier, $.property_identifier)),
        optional(seq(':', field('pattern', $._pattern))),
      ),

    // `[let first, string, ..let rest]` | `[..]` | `let [let head, ..let tail]`
    array_pattern: ($) =>
      seq(
        optional('let'),
        '[',
        commaSep(choice($._pattern, $.rest_pattern)),
        ']',
      ),

    rest_pattern: ($) => seq('..', optional($._pattern)),

    paren_pattern: ($) => prec.dynamic(-1, seq('(', $._pattern, ')')),

    // Bare type as a pattern: `int`, `Status.Active`, `map<string, T[]>`,
    // `(int) -> int`, `"accept"`, `0`, `null`, `PatternRecord?`
    type_pattern: ($) => choice($._type, $.null),

    // ==========================================================================
    // Terminals
    // ==========================================================================

    identifier: (_) => token(IDENT_REGEX),

    number: (_) =>
      token(
        choice(
          /[0-9]+n/, //                       bigint
          /[0-9]+\.[0-9]+([eE][+-]?[0-9]+)?/, // float
          /[0-9]+[eE][+-]?[0-9]+/, //          scientific
          /[0-9]+/, //                         integer
        ),
      ),

    boolean: (_) => choice('true', 'false'),

    null: (_) => 'null',

    string: (_) => token(seq('"', repeat(choice(/[^"\\]/, /\\./)), '"')),

    byte_string: (_) => token(seq('b"', repeat(choice(/[^"\\]/, /\\./)), '"')),

    // Raw strings: hash levels 1..MAX_RAW_STRING_HASHES are enumerated
    // explicitly (same longest-match close-delimiter approach per level).
    // The real lexer allows any count; unbounded nesting would need an
    // external scanner, which this grammar deliberately avoids.
    raw_string: ($) =>
      choice(
        ...Array.from({ length: MAX_RAW_STRING_HASHES }, (_, i) =>
          rawString($, i + 1),
        ),
      ),

    raw_string_content: ($) =>
      repeat1(choice(RAW_CHUNK, alias('"', $.quote))),

    // Backtick strings (BEP-049), 1–3 tick ladders with `${}` interpolation.
    backtick_string: ($) =>
      choice(backtickString($, 1), backtickString($, 2), backtickString($, 3)),

    escape_sequence: (_) => token(prec(2, /\\./)),

    // BEP-049 §4: `${...}` holds a block expression — statements plus an
    // optional trailing expression — or one of the §5 block-tag forms.
    interpolation: ($) =>
      seq(
        '${',
        choice(
          repeat1($._statement),
          $.template_for_open,
          $.template_if_open,
          $.template_else_if,
          alias('else', $.template_else),
          alias('endif', $.template_endif),
          alias('endfor', $.template_endfor),
        ),
        '}',
      ),

    template_for_open: ($) =>
      seq(
        'for',
        '(',
        choice('let', 'const'),
        field('pattern', $._pattern),
        'in',
        field('iterable', $._expression),
        ')',
      ),

    template_if_open: ($) => seq('if', field('condition', $.parenthesized_expression)),

    template_else_if: ($) =>
      seq('else', 'if', field('condition', $.parenthesized_expression)),

    comment: (_) => token(seq('//', /[^\n]*/)),

    block_comment: (_) => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),
  },
});
