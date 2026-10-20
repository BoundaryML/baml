/*
Language: BAML
Description: BAML is a language for building reliable LLM functions (BoundaryML).
Website: https://www.boundaryml.com
Category: misc
*/

/*
highlight.js third-party language definition for BAML.

Single source of truth: typescript2/pkg-grammar-hljs/src/baml.js in the BAML
monorepo (https://github.com/BoundaryML/baml). The keyword lists and literal
forms below are derived from the real lexer
(baml_language/crates/baml_compiler_lexer/src/tokens.rs) and the TextMate
grammar (typescript2/pkg-grammar/src/baml.ts).

Notable BAML shapes this definition covers:
  - declaration blocks: class / enum / interface / function / client<llm> /
    generator / retry_policy / template_string / test / testset / type
  - deprecated hash strings #"..."# (1-3 hash levels) with Jinja {{ ... }} /
    {% ... %} / {# ... #} template markup inside
  - backtick strings `...` (1-3 tick levels) with ${ ... } interpolation
  - byte strings b"..." with \xHH escapes, raw strings r"..."
  - string escapes, matching the compiler
  - attributes @name(...) and @@name(...) as meta
  - env.VAR environment references
  - int / float / bigint (42n) numeric literals

No `illegal` patterns are used (BAML prompts contain arbitrary prose, so any
illegal pattern would be a false-positive machine); `result.illegal` is always
false for this language.
*/

/** @type {import('highlight.js').LanguageFn} */
export default function baml(hljs) {
  // Identifier as the BAML lexer defines its `Word` token
  // (baml_compiler_lexer/src/tokens.rs): first char is a letter or underscore,
  // continuation chars add digits and hyphens (`gpt-4o` is one identifier).
  // Names may be joined by `$` into segments (`ExtractResume$render_prompt`),
  // and a leading `$` marks the special $-prefixed form (`$stream`).
  const IDENT = /\$?[A-Za-z_][A-Za-z0-9_-]*(?:\$[A-Za-z_][A-Za-z0-9_-]*)*/;

  const KEYWORDS = {
    // Keywords are plain words, but identifiers may contain hyphens and
    // $-joined segments; matching the full lexer Word shape here keeps
    // `catch-all-handler`, `gpt-4o`, `for$each`, or `$stream` from being read
    // as a keyword plus trailing junk (keywords themselves contain no `-`/`$`,
    // so real keywords still match exactly).
    $pattern: /\$?[A-Za-z_][A-Za-z0-9_-]*(?:\$[A-Za-z_][A-Za-z0-9_-]*)*/,
    built_in: ['env', 'root', 'baml'],
    keyword: [
      // top-level declaration keywords (lexer TokenKind)
      'class',
      'enum',
      'interface',
      'implements',
      'implement',
      'extends',
      'requires',
      'function',
      'client',
      'generator',
      'test',
      'testset|3',
      'retry_policy|10',
      'template_string|10',
      'type_builder|10',
      'type',
      // control flow keywords
      'if',
      'else',
      'for',
      'while',
      'let',
      'const',
      'in',
      'break',
      'continue',
      'return',
      'throw',
      'match',
      'catch',
      'catch_all|3',
      'throws',
      'spawn|2',
      'await',
      'defer|2',
      // other keywords
      'watch|2',
      'instanceof',
      'is',
      'dynamic',
      // contextual keywords (`x.as<T>`, `spawn ... with`, `test ... with`)
      'as',
      'with',
    ],
    literal: ['true', 'false', 'null'],
    type: [
      'int',
      'float',
      'bigint',
      'string',
      'bool',
      'image',
      'audio',
      'map',
      'json',
      'unknown',
      'never',
      'Self',
    ],
    'variable.language': ['self', '_'],
  };

  // --- Jinja template markup (inside prompt / template_string bodies) -------

  const JINJA_KEYWORDS = {
    $pattern: /[A-Za-z_][A-Za-z0-9_]*/,
    built_in: ['ctx', 'env', '_'],
    keyword: [
      'for',
      'endfor',
      'if',
      'elif',
      'else',
      'endif',
      'in',
      'set',
      'and',
      'or',
      'not',
      'filter',
      'endfilter',
      'macro',
      'endmacro',
      'raw',
      'endraw',
    ],
    literal: ['true', 'false', 'null'],
  };

  const JINJA_INNER_STRING = {
    scope: 'string',
    variants: [
      { begin: /"/, end: /"/ },
      { begin: /'/, end: /'/ },
    ],
  };

  const JINJA_NUMBER = {
    match: /\b\d+(?:\.\d+)?\b/,
    relevance: 0,
    scope: 'number',
  };

  const JINJA_COMMENT = {
    begin: /\{#/,
    end: /#\}/,
    scope: 'comment',
  };

  // {{ expression }}
  const JINJA_EXPRESSION = {
    begin: /\{\{/,
    contains: [JINJA_INNER_STRING, JINJA_NUMBER],
    end: /\}\}/,
    keywords: JINJA_KEYWORDS,
    scope: 'template-variable',
  };

  // {% statement %}
  const JINJA_TAG = {
    begin: /\{%/,
    contains: [JINJA_INNER_STRING, JINJA_NUMBER],
    end: /%\}/,
    keywords: JINJA_KEYWORDS,
    scope: 'template-tag',
  };

  const JINJA_MODES = [JINJA_COMMENT, JINJA_TAG, JINJA_EXPRESSION];

  // --- Strings ---------------------------------------------------------------

  // Raw strings: #"..."#, ##"..."##, ###"..."###. The lexer allows any number
  // of hashes; 3 levels cover realistic code. Prompt bodies use these, so the
  // Jinja modes live inside. A raw string opener is a strong BAML signal.
  const rawString = (hashes) => {
    const h = '#'.repeat(hashes);
    return {
      begin: h + '"',
      contains: JINJA_MODES,
      end: '"' + h,
      relevance: 5,
      scope: 'string',
    };
  };
  // Longest opener first so ###" is not consumed as #" with leading hashes.
  const RAW_STRINGS = [rawString(3), rawString(2), rawString(1)];

  // Escapes, matching the compiler (baml_base::escape). Every literal kind
  // shares one set; strings add ASCII \xHH and \uHHHH / \u{H...}, byte strings
  // take any \xHH. Any other backslash pair is a compile error: it is consumed
  // without a scope (no `illegal`, see above) so it stays plain string text.
  const STRING_ESCAPE = {
    match:
      /\\(?:[ntr0bf"\\'/`$]|x[0-7][0-9A-Fa-f]|u(?:[0-9A-Fa-f]{4}|\{[0-9A-Fa-f](?:_?[0-9A-Fa-f])*\}))/,
    relevance: 0,
    scope: 'char.escape',
  };
  const BYTE_ESCAPE = {
    match: /\\(?:[ntr0bf"\\'/`$]|x[0-9A-Fa-f]{2})/,
    relevance: 0,
    scope: 'char.escape',
  };
  const OTHER_BACKSLASH = { match: /\\./, relevance: 0 };

  // Byte string: b"...".
  const BYTE_STRING = {
    begin: /\bb"/,
    contains: [BYTE_ESCAPE, OTHER_BACKSLASH],
    end: /"/,
    scope: 'string',
  };

  // Raw string: r"...". The body is verbatim, but a backslash keeps the next
  // character (including `"`) from closing the literal.
  const RAW_QUOTED_STRING = {
    begin: /\br"/,
    contains: [OTHER_BACKSLASH],
    end: /"/,
    scope: 'string',
  };

  // Plain double-quoted string. Prompt bodies may also be plain quoted
  // strings, so Jinja markup is recognized here too (harmless elsewhere).
  const QUOTED_STRING = {
    begin: /"/,
    contains: [STRING_ESCAPE, OTHER_BACKSLASH, ...JINJA_MODES],
    end: /"/,
    relevance: 0,
    scope: 'string',
  };

  // ${ ... } interpolation inside backtick strings. The interpolated
  // expression may itself contain braces (`${if ok { "x" } else { "y" }}`), so
  // a self-recursive brace-tracking mode keeps the interpolation open until
  // its own balancing `}`. `contains` for both is filled in below (after the
  // modes they reference exist).
  const NESTED_BRACES = {
    begin: /\{/,
    contains: [],
    end: /\}/,
    keywords: KEYWORDS,
  };

  const SUBST = {
    begin: /\$\{/,
    contains: [],
    end: /\}/,
    keywords: KEYWORDS,
    scope: 'subst',
  };

  // Backtick strings: `...`, ``...``, ```...``` (multi-tick delimiters let a
  // shorter tick run appear as content). Same escapes as quoted strings.
  const backtickString = (ticks) => {
    const t = '`'.repeat(ticks);
    return {
      begin: t,
      contains: [STRING_ESCAPE, OTHER_BACKSLASH, SUBST],
      end: t,
      scope: 'string',
    };
  };
  const BACKTICK_STRINGS = [
    backtickString(3),
    backtickString(2),
    backtickString(1),
  ];

  // --- Numbers -----------------------------------------------------------------

  // bigint (42n) before float (1.5, 1e10, 1.5e-3) before integer, mirroring the
  // lexer's ordering. No leading-dot floats in BAML (`.5` is not a float).
  const NUMBER = {
    relevance: 0,
    scope: 'number',
    variants: [
      { match: /\b\d+n/ },
      { match: /\b\d+(?:\.\d+)?[eE][+-]?\d+/ },
      { match: /\b\d+\.\d+/ },
      { match: /\b\d+\b/ },
    ],
  };

  // --- Attributes ----------------------------------------------------------------

  // Field attributes @alias(...) / @assert / @stream.done and block attributes
  // @@dynamic / @@alias(...). Dotted paths are part of the attribute name.
  const ATTRIBUTE = {
    scope: 'meta',
    variants: [
      {
        match: /@@\$?[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*/,
        relevance: 5,
      },
      {
        match: /@\$?[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*/,
        relevance: 0,
      },
    ],
  };

  // --- env.VAR references ------------------------------------------------------

  const ENV_VAR = {
    match: [/\benv\b/, /\./, IDENT],
    scope: { 1: 'built_in', 3: 'variable.constant' },
  };

  // --- Declarations (titles) -----------------------------------------------------

  // client<llm> Name { ... }  — the single strongest BAML signal.
  const CLIENT_LLM_DECL = {
    match: [/\bclient\b/, /\s*<\s*/, IDENT, /\s*>\s*/, IDENT],
    relevance: 10,
    scope: { 1: 'keyword', 3: 'type', 5: 'title.class' },
  };

  // client Name { ... } (no <llm> type argument)
  const CLIENT_DECL = {
    match: [/\bclient\b/, /\s+/, IDENT, /(?=\s*\{)/],
    scope: { 1: 'keyword', 3: 'title.class' },
  };

  // class / enum / interface Name
  const TYPE_DECL = {
    match: [/\b(?:class|enum|interface)\b/, /\s+/, IDENT],
    scope: { 1: 'keyword', 3: 'title.class' },
  };

  // type Alias = ... (also `type Item` associated types in interfaces)
  const TYPE_ALIAS_DECL = {
    match: [/\btype\b/, /\s+/, IDENT],
    scope: { 1: 'keyword', 3: 'title.class' },
  };

  // function Name(...) -> Ret
  const FUNCTION_DECL = {
    match: [/\bfunction\b/, /\s+/, IDENT],
    scope: { 1: 'keyword', 3: 'title.function' },
  };

  // template_string Name(...) #"..."#
  const TEMPLATE_STRING_DECL = {
    match: [/\btemplate_string\b/, /\s+/, IDENT],
    relevance: 10,
    scope: { 1: 'keyword', 3: 'title.function' },
  };

  // retry_policy / generator / testset / test Name { ... }
  const NAMED_BLOCK_DECL = {
    match: [/\b(?:retry_policy|generator|testset|test)\b/, /\s+/, IDENT],
    scope: { 1: 'keyword', 3: 'title' },
  };

  const ARROW = {
    match: /->|=>/,
    relevance: 0,
    scope: 'operator',
  };

  // Shared contents of an interpolated expression. The list includes
  // NESTED_BRACES, and NESTED_BRACES reuses the list, so arbitrarily deep
  // `{ ... }` nesting inside `${ ... }` is tracked and only the balancing `}`
  // closes the interpolation.
  const INTERPOLATION_CONTAINS = [
    hljs.C_LINE_COMMENT_MODE,
    hljs.C_BLOCK_COMMENT_MODE,
    QUOTED_STRING,
    NUMBER,
    ENV_VAR,
    NESTED_BRACES,
  ];

  NESTED_BRACES.contains = INTERPOLATION_CONTAINS;
  SUBST.contains = INTERPOLATION_CONTAINS;

  return {
    aliases: ['baml'],
    contains: [
      hljs.C_LINE_COMMENT_MODE,
      hljs.C_BLOCK_COMMENT_MODE,
      ...RAW_STRINGS,
      BYTE_STRING,
      RAW_QUOTED_STRING,
      ...BACKTICK_STRINGS,
      QUOTED_STRING,
      CLIENT_LLM_DECL,
      CLIENT_DECL,
      TYPE_DECL,
      TYPE_ALIAS_DECL,
      FUNCTION_DECL,
      TEMPLATE_STRING_DECL,
      NAMED_BLOCK_DECL,
      ATTRIBUTE,
      ENV_VAR,
      NUMBER,
      ARROW,
    ],
    keywords: KEYWORDS,
    name: 'BAML',
  };
}
