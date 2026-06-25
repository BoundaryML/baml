import * as tm from "tmlanguage-generator";

// Authored TextMate scopes used by the BAML grammar. Widen as rules are added.
export type BamlScope = string;

type Grammar = tm.Grammar<BamlScope>;
type Rule = tm.Rule<BamlScope>;
type IncludeRule = tm.IncludeRule<BamlScope>;

// Rules are referenced directly inside a `patterns` array; tmlanguage-generator
// hoists each one into the emitted `repository` (keyed by its `key`) and
// replaces the reference with `{ include: "#<key>" }`.

// An identifier as the BAML lexer defines its `Word` token
// (baml_compiler_lexer/src/tokens.rs):
//
//   $-prefixed: \$[a-zA-Z_][a-zA-Z0-9_]*
//   normal:     [a-zA-Z_][a-zA-Z0-9_-]*(\$[a-zA-Z_][a-zA-Z0-9_-]*)*
//
// First char is a letter or underscore; continuation chars add digits and
// hyphens (so `gpt-4o` is one identifier). Names may be joined by `$` into
// segments (e.g. `ExtractResume$render_prompt`), and a leading `$` marks the
// special `$`-prefixed form (e.g. `$stream`). Raw oniguruma source, interpolated
// into a rule's `match` / `begin` / `end`.
const IDENT = String.raw`\$?[A-Za-z_][A-Za-z0-9_-]*(?:\$[A-Za-z_][A-Za-z0-9_-]*)*`;
const ACCESSOR = String.raw`\s*\.\s*`;
const DOTTED_IDENT = String.raw`${IDENT}(?:${ACCESSOR}${IDENT})*`;

// --- Builtins --------------------------------------------------------------
//
// Reserved names, kept as arrays so they are a single source of truth shared by
// the rules below (and reusable elsewhere).

// Primitive types. Highlighted as builtin types, distinct from user types.
const BUILTIN_TYPES = [
  "int",
  "float",
  "string",
  "bool",
  "image",
  "audio",
  "map",
  "json",
];

// Reserved root namespaces. Highlighted as builtins when they lead a path.
const BUILTIN_NAMESPACES = ["root", "baml"];

// Non-capturing alternation of literal words: ["a", "b"] -> "(?:a|b)". The names
// above are plain identifiers, so they need no regex escaping.
const oneOf = (words: string[]) => `(?:${words.join("|")})`;

const TOP_LEVEL_ITEMS = [
  "client",
  "retry_policy",
  "generator",
  "class",
  "function",
  "testset",
  "test",
  "type",
];

const TOP_LEVEL_ITEM_START = String.raw`^\s*${oneOf(TOP_LEVEL_ITEMS)}\b`;

// `root` / `baml` are reserved root namespaces; when one leads a path it is a
// builtin, not a user namespace. Capture re-tokenization keeps the original
// line coordinates, so this intentionally has no start anchor and should only
// be used for the leading path segment capture.
const namespaceRoot: Rule = {
  key: "namespace-root",
  scope: "support.other.namespace.baml",
  match: String.raw`\b${oneOf(BUILTIN_NAMESPACES)}\b`,
};

// --- Comments --------------------------------------------------------------

const lineComment: Rule = {
  key: "line-comment",
  scope: "comment.line.double-slash.baml",
  match: "//.*$",
};

const blockComment: Rule = {
  key: "block-comment",
  scope: "comment.block.baml",
  begin: "/\\*",
  end: "\\*/",
};

const comments: Rule = {
  key: "comments",
  patterns: [lineComment, blockComment],
};

// Generic `{ ... }` block for code-ish regions.
const block: Rule = {
  key: "block",
  scope: "meta.block.baml",
  begin: String.raw`\{`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.block.begin.baml" },
  },
  end: String.raw`\}`,
  endCaptures: {
    "0": { scope: "punctuation.definition.block.end.baml" },
  },
  patterns: [comments],
};

// --- Literals --------------------------------------------------------------
//
// Derived from the BAML lexer (baml_compiler_lexer/src/tokens.rs). Every literal
// is also a valid type expression in BAML (literal types: `1 | 2`, `"a" | "b"`,
// `true`), so the `literal` group below is wired into `typeExpression` as well.

// `true` / `false` are not keywords in the lexer -- they lex as plain `Word`
// tokens and the parser gives them meaning. We still colour them as language
// constants, which is what every mainstream grammar does.
const booleanLiteral: Rule = {
  key: "boolean-literal",
  scope: "constant.language.boolean.baml",
  match: String.raw`\b(?:true|false)\b`,
};

// `null` is a literal value (and literal type), not a primitive type -- it lexes
// as a plain `Word`, like the booleans, so it is a language constant too.
const nullLiteral: Rule = {
  key: "null-literal",
  scope: "constant.language.null.baml",
  match: String.raw`\bnull\b`,
};

// Bigint: `[0-9]+n` (lexer `BigintLiteral`). No trailing boundary so it mirrors
// the lexer's maximal munch (`42name` -> `42n` + `ame`); the leading `\b` keeps
// it from firing inside an identifier (`a42n` is one `Word`).
const bigintLiteral: Rule = {
  key: "bigint-literal",
  scope: "constant.numeric.bigint.baml",
  match: String.raw`\b[0-9]+n`,
};

// Float (lexer `FloatLiteral`), the union of its two regexes:
//   [0-9]+\.[0-9]+                     plain decimal (1.0, 3.14)
//   [0-9]+(\.[0-9]+)?[eE][+-]?[0-9]+   scientific  (1e10, 2e+5, 1.5e-3)
// Note there is no leading-dot form: `.5` is not a float in BAML.
const floatLiteral: Rule = {
  key: "float-literal",
  scope: "constant.numeric.float.baml",
  match: String.raw`\b[0-9]+(?:\.[0-9]+[eE][+-]?[0-9]+|\.[0-9]+|[eE][+-]?[0-9]+)`,
};

// Integer: `[0-9]+` (lexer `IntegerLiteral`). The trailing `\b` stops it from
// claiming the digits of a bigint (`42n`) or a member access target.
const integerLiteral: Rule = {
  key: "integer-literal",
  scope: "constant.numeric.integer.baml",
  match: String.raw`\b[0-9]+\b`,
};

// Ordered numeric group: bigint and float must be tried before integer, or the
// integer rule would peel off their leading digits.
const numericLiteral: Rule = {
  key: "numeric-literal",
  patterns: [bigintLiteral, floatLiteral, integerLiteral],
};

// `\<char>` escape inside quoted strings. The lexer treats a quote preceded by
// an odd number of backslashes as still inside the string, which this matches:
// the escape consumes `\"` before the closing-quote rule can see it.
const stringEscape: Rule = {
  key: "string-escape",
  scope: "constant.character.escape.baml",
  match: String.raw`\\.`,
};

// Plain double-quoted string: `"..."`.
const stringLiteral: Rule = {
  key: "string-literal",
  scope: "string.quoted.double.baml",
  begin: String.raw`"`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.string.begin.baml" },
  },
  end: String.raw`"`,
  endCaptures: {
    "0": { scope: "punctuation.definition.string.end.baml" },
  },
  patterns: [stringEscape],
};

// Raw and backtick strings use a balanced run of delimiters (`##"..."##`,
// ```` ```...``` ````). A single backreference rule would be the natural fit,
// but tmlanguage-generator validates each regex standalone and Oniguruma rejects
// the unresolvable `\1` -- so we enumerate one rule per delimiter count up to
// `MAX_DELIMITER`. The lexer allows any count; 8 covers every realistic string.
const MAX_DELIMITER = 8;

// Each rule pins exactly N hashes: `(?<!#)` stops a shorter rule from matching
// inside a longer opener, and the trailing `"` bounds the right side.
function rawStringRules(max: number): Rule[] {
  return Array.from({ length: max }, (_, i) => max - i).map((n) => ({
    key: `raw-string-${n}`,
    scope: "string.quoted.raw.baml",
    begin: String.raw`(?<!#)(#{${n}})"`,
    beginCaptures: {
      "1": { scope: "punctuation.definition.string.begin.baml" },
    },
    end: String.raw`"(#{${n}})`,
    endCaptures: {
      "1": { scope: "punctuation.definition.string.end.baml" },
    },
  }));
}

const rawStringLiteral: Rule = {
  key: "raw-string",
  patterns: rawStringRules(MAX_DELIMITER),
};

// Backticks have no terminator character, so each rule is pinned to exactly N
// with lookarounds on both sides (`(?<!\`)...(?!\`)`); a shorter run inside (a
// lone `` ` ``) is just string content. Interpolation (`${...}`) is not split
// out yet -- the whole body is string.
function backtickStringRules(max: number): Rule[] {
  return Array.from({ length: max }, (_, i) => max - i).map((n) => ({
    key: `backtick-string-${n}`,
    scope: "string.interpolated.baml",
    begin: String.raw`(?<!\`)(\`{${n}})(?!\`)`,
    beginCaptures: {
      "1": { scope: "punctuation.definition.string.begin.baml" },
    },
    end: String.raw`(?<!\`)(\`{${n}})(?!\`)`,
    endCaptures: {
      "1": { scope: "punctuation.definition.string.end.baml" },
    },
  }));
}

const backtickStringLiteral: Rule = {
  key: "backtick-string",
  patterns: backtickStringRules(MAX_DELIMITER),
};

// Every literal form, in one reusable group.
const literal: Rule = {
  key: "literal",
  patterns: [
    booleanLiteral,
    nullLiteral,
    numericLiteral,
    stringLiteral,
    rawStringLiteral,
    backtickStringLiteral,
  ],
};

const expression: IncludeRule = {
  key: "expression",
  patterns: [],
};

const parenthesizedExpression: Rule = {
  key: "parenthesized-expression",
  scope: "meta.group.expression.baml",
  begin: String.raw`\(`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.group.begin.baml" },
  },
  end: String.raw`\)`,
  endCaptures: {
    "0": { scope: "punctuation.definition.group.end.baml" },
  },
  patterns: [comments, expression],
};

const conditionExpression: IncludeRule = {
  key: "condition-expression",
  patterns: [],
};

const typeExpression: IncludeRule = {
  key: "type-expression",
  patterns: [],
};

const blockContents: IncludeRule = {
  key: "block-contents",
  patterns: [],
};

const codeBlock: Rule = {
  key: "code-block",
  scope: "meta.block.code.baml",
  begin: String.raw`\{`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.block.begin.baml" },
  },
  end: String.raw`\}`,
  endCaptures: {
    "0": { scope: "punctuation.definition.block.end.baml" },
  },
  patterns: [comments, blockContents],
};

const arrayExpression: Rule = {
  key: "array-expression",
  scope: "meta.expression.array.baml",
  begin: String.raw`\[`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.array.begin.baml" },
  },
  end: String.raw`\]`,
  endCaptures: {
    "0": { scope: "punctuation.definition.array.end.baml" },
  },
  patterns: [
    comments,
    expression,
    {
      key: "array-expression-comma",
      scope: "punctuation.separator.comma.baml",
      match: String.raw`,`,
    },
  ],
};

const constructorExpression: Rule = {
  key: "constructor-expression",
  scope: "meta.constructor.expression.baml",
  begin: String.raw`\b${DOTTED_IDENT}\b\s*(?=\{)`,
  beginCaptures: {
    "0": {
      patterns: [
        {
          key: "constructor-accessor",
          scope: "punctuation.accessor.baml",
          match: String.raw`\.`,
        },
        {
          key: "constructor-type",
          scope: "entity.name.type.baml",
          match: String.raw`\b${IDENT}\b`,
        },
      ],
    },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    {
      key: "constructor-body",
      scope: "meta.constructor.body.baml",
      begin: String.raw`\{`,
      beginCaptures: {
        "0": {
          scope: "punctuation.definition.constructor.body.begin.baml",
        },
      },
      end: String.raw`\}`,
      endCaptures: {
        "0": {
          scope: "punctuation.definition.constructor.body.end.baml",
        },
      },
      patterns: [
        comments,
        {
          key: "constructor-field",
          scope: "meta.constructor.field.baml",
          begin: String.raw`\b(${IDENT})\s*(:)`,
          beginCaptures: {
            "1": { scope: "variable.other.property.baml" },
            "2": { scope: "punctuation.separator.colon.baml" },
          },
          end: String.raw`(?=,|\})`,
          patterns: [comments, expression],
        },
        {
          key: "constructor-comma",
          scope: "punctuation.separator.comma.baml",
          match: String.raw`,`,
        },
      ],
    },
  ],
};

const environmentExpressionRoot: Rule = {
  key: "environment-expression-root",
  scope: "support.other.namespace.baml",
  match: String.raw`\benv\b`,
};

const selfExpressionRoot: Rule = {
  key: "self-expression-root",
  scope: "variable.language.self.baml",
  match: String.raw`\bself\b`,
};

const expressionRootIdentifier: Rule = {
  key: "expression-root-identifier",
  scope: "variable.other.readwrite.baml",
  match: String.raw`\b${IDENT}\b`,
};

const expressionAccessor: Rule = {
  key: "expression-accessor",
  scope: "punctuation.accessor.baml",
  match: String.raw`\.`,
};

const expressionMemberIdentifier: Rule = {
  key: "expression-member-identifier",
  scope: "variable.other.readwrite.baml",
  match: String.raw`\b${IDENT}\b`,
};

const expressionPathRootCapture = {
  patterns: [
    namespaceRoot,
    environmentExpressionRoot,
    selfExpressionRoot,
    expressionRootIdentifier,
  ],
};

const expressionPathMemberCapture = {
  patterns: [expressionAccessor, expressionMemberIdentifier],
};

const functionCallExpression: Rule = {
  key: "function-call-expression",
  scope: "meta.function-call.baml",
  begin: String.raw`\b(?:(${IDENT})\s*(\.)\s*)?((?:${IDENT}${ACCESSOR})*)(${IDENT})\b(?=\s*(?:<[^(){};]*>\s*)?\()`,
  beginCaptures: {
    "1": expressionPathRootCapture,
    "2": { scope: "punctuation.accessor.baml" },
    "3": expressionPathMemberCapture,
    "4": { scope: "entity.name.function.baml" },
  },
  end: String.raw`(?<=\))`,
  patterns: [
    comments,
    {
      key: "function-call-type-arguments",
      scope: "meta.type-arguments.baml",
      begin: String.raw`<`,
      beginCaptures: {
        "0": { scope: "punctuation.definition.type-arguments.begin.baml" },
      },
      end: String.raw`>(?=\s*\()`,
      endCaptures: {
        "0": { scope: "punctuation.definition.type-arguments.end.baml" },
      },
      patterns: [comments, typeExpression],
    },
    {
      key: "function-call-arguments",
      scope: "meta.function-call.arguments.baml",
      begin: String.raw`\(`,
      beginCaptures: {
        "0": { scope: "punctuation.definition.arguments.begin.baml" },
      },
      end: String.raw`\)`,
      endCaptures: {
        "0": { scope: "punctuation.definition.arguments.end.baml" },
      },
      patterns: [
        comments,
        expression,
        {
          key: "function-call-argument-comma",
          scope: "punctuation.separator.comma.baml",
          match: String.raw`,`,
        },
      ],
    },
  ],
};

const dottedExpression: Rule = {
  key: "dotted-expression",
  scope: tm.meta,
  match: String.raw`\b(${IDENT})((?:${ACCESSOR}${IDENT})*)\b`,
  captures: {
    "1": expressionPathRootCapture,
    "2": expressionPathMemberCapture,
  },
};

const spawnExpression: Rule = {
  key: "spawn-expression",
  scope: "meta.expression.spawn.baml",
  begin: String.raw`\b(spawn)\b`,
  beginCaptures: {
    "1": { scope: "keyword.operator.spawn.baml" },
  },
  end: String.raw`(?<=\})|(?=,|;|$)`,
  patterns: [
    comments,
    {
      key: "spawn-header",
      scope: "meta.spawn.header.baml",
      begin: String.raw`\G\s*`,
      end: String.raw`(?=\{)`,
      patterns: [
        comments,
        {
          key: "spawn-with-keyword",
          scope: "keyword.operator.with.baml",
          match: String.raw`\bwith\b`,
        },
        conditionExpression,
        {
          key: "spawn-header-comma",
          scope: "punctuation.separator.comma.baml",
          match: String.raw`,`,
        },
      ],
    },
    codeBlock,
  ],
};

const awaitExpression: Rule = {
  key: "await-expression",
  scope: "meta.expression.await.baml",
  begin: String.raw`\b(await)\b`,
  beginCaptures: {
    "1": { scope: "keyword.operator.await.baml" },
  },
  end: String.raw`(?=,|\}|;|$)`,
  patterns: [comments, expression],
};

const throwExpression: Rule = {
  key: "throw-expression",
  scope: "meta.expression.throw.baml",
  begin: String.raw`\b(throw)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.flow.throw.baml" },
  },
  end: String.raw`(?=,|\}|;|$)`,
  patterns: [comments, expression],
};

const catchExpression: Rule = {
  key: "catch-expression",
  scope: "meta.expression.catch.baml",
  begin: String.raw`\b(catch|catch_all)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.exception.catch.baml" },
  },
  end: String.raw`(?<=\})|(?=,|;|$)`,
  patterns: [],
};

const expressionOperator: Rule = {
  key: "expression-operator",
  patterns: [
    {
      key: "expression-arrow-operator",
      scope: "keyword.operator.arrow.baml",
      match: String.raw`=>|->`,
    },
    {
      key: "expression-compound-assignment-operator",
      scope: "keyword.operator.assignment.baml",
      match: String.raw`<<=|>>=|\+=|-=|\*=|/=|%=|&=|\|=|\^=`,
    },
    {
      key: "expression-nullish-operator",
      scope: "keyword.operator.nullish.baml",
      match: String.raw`\?\?`,
    },
    {
      key: "expression-logical-operator",
      scope: "keyword.operator.logical.baml",
      match: String.raw`&&|\|\||!`,
    },
    {
      key: "expression-equality-operator",
      scope: "keyword.operator.comparison.baml",
      match: String.raw`==|!=`,
    },
    {
      key: "expression-shift-operator",
      scope: "keyword.operator.bitwise.shift.baml",
      match: String.raw`<<|>>`,
    },
    {
      key: "expression-comparison-operator",
      scope: "keyword.operator.comparison.baml",
      match: String.raw`<=|>=|<|>`,
    },
    {
      key: "expression-bitwise-operator",
      scope: "keyword.operator.bitwise.baml",
      match: String.raw`&|\^|~|\|`,
    },
    {
      key: "expression-arithmetic-operator",
      scope: "keyword.operator.arithmetic.baml",
      match: String.raw`\+\+|--|\+|-|\*|/|%`,
    },
    {
      key: "expression-assignment-operator",
      scope: "keyword.operator.assignment.baml",
      match: String.raw`=`,
    },
    {
      key: "expression-spread-operator",
      scope: "keyword.operator.spread.baml",
      match: String.raw`\.\.\.`,
    },
    {
      key: "expression-accessor-operator",
      scope: "punctuation.accessor.baml",
      match: String.raw`\?\.|\.|\$`,
    },
  ],
};

// --- Types -----------------------------------------------------------------

const primitiveType: Rule = {
  key: "primitive-type",
  scope: "support.type.primitive.baml",
  match: String.raw`\b${oneOf(BUILTIN_TYPES)}\b`,
};

// One segment of a namespace path, and the `.` between segments. These only
// re-tokenize the prefix captured by `typeReference` below.
const namespaceSegment: Rule = {
  key: "namespace-segment",
  scope: "entity.name.namespace.baml",
  match: String.raw`\b${IDENT}\b`,
};

const namespaceSeparator: Rule = {
  key: "namespace-separator",
  scope: "punctuation.accessor.baml",
  match: String.raw`\.`,
};

// A type reference, namespaced or not: `Foo`, or `name.space.Foo`. One rule
// covers both. The first namespace segment is captured on its own (group 1) so
// the builtin roots `root` / `baml` can be coloured only in leading position;
// group 2 is its separator, group 3 the remaining `space.` prefix, and group 4
// the final type name. For a bare `Foo`, only group 4 is present.
const typeReference: Rule = {
  key: "type-reference",
  // The whole reference gets no single colour; only its captures do.
  scope: tm.meta,
  match: String.raw`\b(?:(${IDENT})\s*(\.)\s*)?((?:${IDENT}${ACCESSOR})*)(${IDENT})\b`,
  captures: {
    "1": { patterns: [namespaceRoot, namespaceSegment] },
    "2": { scope: "punctuation.accessor.baml" },
    "3": { patterns: [namespaceSegment, namespaceSeparator] },
    "4": { scope: "entity.name.type.baml" },
  },
};

// `?` (optional) and `|` (union) are operators, not punctuation -- themes that
// colour operators distinctly will then set them apart from the brackets.
const optionalOperator: Rule = {
  key: "optional-operator",
  scope: "keyword.operator.optional.baml",
  match: String.raw`\?`,
};

const unionOperator: Rule = {
  key: "union-operator",
  scope: "keyword.operator.type.baml",
  match: String.raw`\|`,
};

const typeArrowOperator: Rule = {
  key: "type-arrow-operator",
  scope: "keyword.operator.arrow.baml",
  match: String.raw`->`,
};

const typeThrowsOperator: Rule = {
  key: "type-throws-operator",
  scope: "keyword.operator.throws.baml",
  match: String.raw`\bthrows\b`,
};

// Brackets, angles, and commas are pure punctuation.
const typePunctuation: Rule = {
  key: "type-punctuation",
  scope: "punctuation.definition.type.baml",
  match: String.raw`[\[\]<>,()]`,
};

const typeColon: Rule = {
  key: "type-colon",
  scope: "punctuation.separator.colon.baml",
  match: String.raw`:`,
};

const semicolon: Rule = {
  key: "semicolon",
  scope: "punctuation.terminator.statement.baml",
  match: String.raw`;`,
};

// A field's type. Order matters: primitives and literals are claimed before the
// catch-all `typeReference`, so `string` is a primitive and `true` / `"active"`
// / `42` are literal types rather than bare identifiers.
typeExpression.patterns = [
  comments,
  primitiveType,
  literal,
  typeArrowOperator,
  typeThrowsOperator,
  typeReference,
  optionalOperator,
  unionOperator,
  typePunctuation,
  typeColon,
];

const pattern: IncludeRule = {
  key: "pattern",
  patterns: [],
};

const wildcardPattern: Rule = {
  key: "wildcard-pattern",
  scope: "variable.language.wildcard.baml",
  match: String.raw`\b_\b`,
};

const wildcardBindingPattern: Rule = {
  key: "wildcard-binding-pattern",
  scope: "meta.pattern.wildcard.baml",
  match: String.raw`\b(let)\s+(_)\b`,
  captures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "variable.language.wildcard.baml" },
  },
};

const typedBindingPattern: Rule = {
  key: "typed-binding-pattern",
  scope: "meta.pattern.binding.baml",
  match: String.raw`\b(let)\s+(${IDENT})\b\s*(:)`,
  captures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "variable.other.binding.baml" },
    "3": { scope: "punctuation.separator.colon.baml" },
  },
};

const bareBindingPattern: Rule = {
  key: "bare-binding-pattern",
  scope: "meta.pattern.binding.baml",
  match: String.raw`\b(let)\s+(${IDENT})\b`,
  captures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "variable.other.binding.baml" },
  },
};

const classDestructurePattern: Rule = {
  key: "class-destructure-pattern",
  scope: "meta.pattern.destructure.class.baml",
  begin: String.raw`\b(?:(let)\s+)?(${DOTTED_IDENT})\b\s*(?=(?:<[^{}]*>\s*)?\{)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { patterns: [typeReference] },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    typeExpression,
    {
      key: "class-destructure-body",
      scope: "meta.pattern.destructure.class.body.baml",
      begin: String.raw`\{`,
      beginCaptures: {
        "0": {
          scope: "punctuation.definition.pattern.destructure.begin.baml",
        },
      },
      end: String.raw`\}`,
      endCaptures: {
        "0": {
          scope: "punctuation.definition.pattern.destructure.end.baml",
        },
      },
      patterns: [
        comments,
        {
          key: "class-destructure-field",
          scope: "meta.pattern.destructure.class.field.baml",
          begin: String.raw`\b(${IDENT})\b\s*(:)?`,
          beginCaptures: {
            "1": { scope: "variable.other.property.baml" },
            "2": { scope: "punctuation.separator.colon.baml" },
          },
          end: String.raw`(?=,|\})`,
          patterns: [comments, pattern],
        },
        {
          key: "class-destructure-comma",
          scope: "punctuation.separator.comma.baml",
          match: String.raw`,`,
        },
      ],
    },
  ],
};

const arrayDestructurePattern: Rule = {
  key: "array-destructure-pattern",
  scope: "meta.pattern.destructure.array.baml",
  begin: String.raw`(?:\b(let)\b\s*)?(\[)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "punctuation.definition.array.begin.baml" },
  },
  end: String.raw`(\])(?:\s*(:)\s*((?:(?!\s*(?:in\b|if\b|=>|=|[,}\)])).)+))?`,
  endCaptures: {
    "1": { scope: "punctuation.definition.array.end.baml" },
    "2": { scope: "punctuation.separator.colon.baml" },
    "3": { patterns: [comments, typeExpression] },
  },
  patterns: [
    comments,
    {
      key: "array-destructure-rest-operator",
      scope: "keyword.operator.rest.baml",
      match: String.raw`\.\.`,
    },
    pattern,
    {
      key: "array-destructure-comma",
      scope: "punctuation.separator.comma.baml",
      match: String.raw`,`,
    },
  ],
};

pattern.patterns = [
  comments,
  wildcardBindingPattern,
  typedBindingPattern,
  classDestructurePattern,
  arrayDestructurePattern,
  bareBindingPattern,
  wildcardPattern,
  typeExpression,
];

const ifExpressionEnd = String.raw`(?!\s*else\b)(?:(?<=\})(?=\s*(?:[,);]|$))|(?=,|\)|;|$))`;

const ifExpression: Rule = {
  key: "if-expression",
  scope: "meta.expression.if.baml",
  begin: String.raw`\b(if)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.conditional.baml" },
  },
  end: ifExpressionEnd,
  patterns: [],
};

ifExpression.patterns = [
  comments,
  {
    key: "if-let-pattern",
    scope: "meta.pattern.if-let.baml",
    begin: String.raw`\G\s*(?=let\b)`,
    end: String.raw`(?=\s*=)`,
    patterns: [comments, pattern],
  },
  {
    key: "if-let-assignment-operator",
    scope: "keyword.operator.assignment.baml",
    match: String.raw`=`,
  },
  {
    key: "if-condition",
    scope: "meta.if.condition.baml",
    begin: String.raw`\G(?!\s*else\b)\s*`,
    end: String.raw`(?=\{)`,
    patterns: [comments, conditionExpression],
  },
  codeBlock,
  {
    key: "if-else-clause",
    scope: "meta.else.baml",
    begin: String.raw`\b(else)\b`,
    beginCaptures: {
      "1": { scope: "keyword.control.conditional.baml" },
    },
    end: ifExpressionEnd,
    patterns: [comments, ifExpression, codeBlock],
  },
];

const matchScrutineeGroup: Rule = {
  key: "match-scrutinee-group",
  scope: "meta.match.scrutinee.group.baml",
  begin: String.raw`\(`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.group.begin.baml" },
  },
  end: String.raw`\)`,
  endCaptures: {
    "0": { scope: "punctuation.definition.group.end.baml" },
  },
  patterns: [comments, expression, typeExpression],
};

const matchScrutinee: Rule = {
  key: "match-scrutinee",
  scope: "meta.match.scrutinee.baml",
  begin: String.raw`(?=[^\s\{])`,
  end: String.raw`(?=\{)`,
  patterns: [
    comments,
    literal,
    arrayExpression,
    spawnExpression,
    awaitExpression,
    throwExpression,
    catchExpression,
    expressionOperator,
    functionCallExpression,
    dottedExpression,
  ],
};

const matchArm: Rule = {
  key: "match-arm",
  scope: "meta.match.arm.baml",
  begin: String.raw`(?=\S)(?![,}])`,
  end: String.raw`(?=,|\r?\n|\})`,
  patterns: [
    comments,
    {
      key: "match-arm-pattern",
      scope: "meta.pattern.match.baml",
      begin: String.raw`\G\s*`,
      end: String.raw`(?=\s*(?:if\b|=>))`,
      patterns: [comments, pattern],
    },
    {
      key: "match-arm-guard",
      scope: "meta.match.guard.baml",
      begin: String.raw`\b(if)\b`,
      beginCaptures: {
        "1": { scope: "keyword.control.conditional.baml" },
      },
      end: String.raw`(?=\s*=>)`,
      patterns: [comments, expression],
    },
    {
      key: "match-arm-arrow",
      scope: "keyword.operator.arrow.baml",
      match: String.raw`=>`,
    },
    codeBlock,
    expression,
  ],
};

catchExpression.patterns = [
  comments,
  {
    key: "catch-binding-list",
    scope: "meta.catch.binding-list.baml",
    begin: String.raw`\(`,
    beginCaptures: {
      "0": { scope: "punctuation.definition.catch-binding.begin.baml" },
    },
    end: String.raw`\)`,
    endCaptures: {
      "0": { scope: "punctuation.definition.catch-binding.end.baml" },
    },
    patterns: [
      comments,
      {
        key: "catch-binding",
        scope: "variable.parameter.catch.baml",
        match: String.raw`\b${IDENT}\b`,
      },
      {
        key: "catch-binding-comma",
        scope: "punctuation.separator.comma.baml",
        match: String.raw`,`,
      },
    ],
  },
  {
    key: "catch-block",
    scope: "meta.block.catch.baml",
    begin: String.raw`\{`,
    beginCaptures: {
      "0": { scope: "punctuation.definition.block.begin.baml" },
    },
    end: String.raw`\}`,
    endCaptures: {
      "0": { scope: "punctuation.definition.block.end.baml" },
    },
    patterns: [
      comments,
      matchArm,
      {
        key: "catch-arm-comma",
        scope: "punctuation.separator.comma.baml",
        match: String.raw`,`,
      },
    ],
  },
];

const matchBlock: Rule = {
  key: "match-block",
  scope: "meta.block.match.baml",
  begin: String.raw`\{`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.block.begin.baml" },
  },
  end: String.raw`\}`,
  endCaptures: {
    "0": { scope: "punctuation.definition.block.end.baml" },
  },
  patterns: [
    comments,
    matchArm,
    {
      key: "match-arm-comma",
      scope: "punctuation.separator.comma.baml",
      match: String.raw`,`,
    },
  ],
};

const matchExpression: Rule = {
  key: "match-expression",
  scope: "meta.expression.match.baml",
  begin: String.raw`\b(match)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.match.baml" },
  },
  end: String.raw`(?<=\})|(?=,|;|$)`,
  patterns: [comments, matchScrutineeGroup, matchBlock, matchScrutinee],
};

expression.patterns = [
  literal,
  arrayExpression,
  parenthesizedExpression,
  ifExpression,
  matchExpression,
  spawnExpression,
  constructorExpression,
  awaitExpression,
  throwExpression,
  catchExpression,
  expressionOperator,
  functionCallExpression,
  dottedExpression,
];

conditionExpression.patterns = expression.patterns.filter(
  (rule) => rule !== constructorExpression,
);

const loopStatementEnd = String.raw`(?<=\})(?=\s*(?:;|$))|(?=;|$)`;

const whileStatement: Rule = {
  key: "while-statement",
  scope: "meta.statement.while.baml",
  begin: String.raw`\b(while)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.loop.while.baml" },
  },
  end: loopStatementEnd,
  patterns: [
    comments,
    {
      key: "while-let-pattern",
      scope: "meta.pattern.while-let.baml",
      begin: String.raw`\G\s*(?=let\b)`,
      end: String.raw`(?=\s*=)`,
      patterns: [comments, pattern],
    },
    {
      key: "while-let-assignment-operator",
      scope: "keyword.operator.assignment.baml",
      match: String.raw`=`,
    },
    {
      key: "while-condition",
      scope: "meta.while.condition.baml",
      begin: String.raw`\G\s*`,
      end: String.raw`(?=\{)`,
      patterns: [comments, conditionExpression],
    },
    codeBlock,
  ],
};

const forStatement: Rule = {
  key: "for-statement",
  scope: "meta.statement.for.baml",
  begin: String.raw`\b(for)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.loop.for.baml" },
  },
  end: loopStatementEnd,
  patterns: [
    comments,
    {
      key: "for-parenthesized-in-header",
      scope: "meta.for.header.baml",
      begin: String.raw`\G\s*(\()(?=\s*let\b(?:(?![=;]).)*\bin\b)`,
      beginCaptures: {
        "1": { scope: "punctuation.definition.for-header.begin.baml" },
      },
      end: String.raw`\)(?=\s*\{)`,
      endCaptures: {
        "0": { scope: "punctuation.definition.for-header.end.baml" },
      },
      patterns: [
        comments,
        {
          key: "for-in-pattern",
          scope: "meta.pattern.for-in.baml",
          begin: String.raw`(?=let\b)`,
          end: String.raw`(?=\s+in\b)`,
          patterns: [comments, pattern],
        },
        {
          key: "for-in-keyword",
          scope: "keyword.operator.in.baml",
          match: String.raw`\bin\b`,
        },
        expression,
      ],
    },
    {
      key: "for-parenthesized-c-style-header",
      scope: "meta.for.header.baml",
      begin: String.raw`\G\s*(\()`,
      beginCaptures: {
        "1": { scope: "punctuation.definition.for-header.begin.baml" },
      },
      end: String.raw`\)(?=\s*\{)`,
      endCaptures: {
        "0": { scope: "punctuation.definition.for-header.end.baml" },
      },
      patterns: [
        comments,
        {
          key: "for-c-style-let-initializer",
          scope: "meta.for.initializer.baml",
          begin: String.raw`(?=let\b)`,
          end: String.raw`(?=;)`,
          patterns: [comments, pattern, expression],
        },
        semicolon,
        expression,
      ],
    },
    {
      key: "for-unparenthesized-header",
      scope: "meta.for.header.baml",
      begin: String.raw`\G\s*(?=let\b)`,
      end: String.raw`(?=\{)`,
      patterns: [
        comments,
        {
          key: "for-unparenthesized-in-pattern",
          scope: "meta.pattern.for-in.baml",
          begin: String.raw`(?=let\b)`,
          end: String.raw`(?=\s+in\b)`,
          patterns: [comments, pattern],
        },
        {
          key: "for-unparenthesized-in-keyword",
          scope: "keyword.operator.in.baml",
          match: String.raw`\bin\b`,
        },
        conditionExpression,
      ],
    },
    codeBlock,
  ],
};

// --- Classes ---------------------------------------------------------------

// `<name> <type>` on its own line inside a class body. The colon is optional:
// the parser accepts both `name string` and `name: string` (parse_field eats an
// optional Colon), so scope it only when present.
const field: Rule = {
  key: "field",
  scope: "meta.field.baml",
  begin: String.raw`^\s*(?!@)(${IDENT})\s*(:)?`,
  beginCaptures: {
    "1": { scope: "variable.other.property.baml" },
    "2": { scope: "punctuation.separator.colon.baml" },
  },
  end: String.raw`$`,
  patterns: [comments, typeExpression], // TODO: attributes
};

const functionParameters: Rule = {
  key: "function-parameters",
  scope: "meta.parameters.baml",
  begin: String.raw`\(`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.parameters.begin.baml" },
  },
  end: String.raw`\)`,
  endCaptures: {
    "0": { scope: "punctuation.definition.parameters.end.baml" },
  },
  patterns: [
    comments,
    {
      key: "self-parameter",
      scope: "meta.parameter.baml",
      begin: String.raw`\b(self)\b(?=\s*(?:,|\)))`,
      beginCaptures: {
        "1": { scope: "variable.language.self.baml" },
      },
      end: String.raw`(?=\s*(?:,|\)))`,
      patterns: [comments],
    },
    {
      key: "parameter",
      scope: "meta.parameter.baml",
      begin: String.raw`\b(${IDENT})\s*(:)?`,
      beginCaptures: {
        "1": { scope: "variable.parameter.baml" },
        "2": { scope: "punctuation.separator.colon.baml" },
      },
      end: String.raw`(?=,|\))`,
      patterns: [comments, typeExpression],
    },
    {
      key: "parameter-comma",
      scope: "punctuation.separator.comma.baml",
      match: String.raw`,`,
    },
  ],
};

const functionReturnType: Rule = {
  key: "function-return-type",
  scope: "meta.return-type.baml",
  begin: String.raw`(->)`,
  beginCaptures: {
    "1": { scope: "keyword.operator.arrow.baml" },
  },
  end: String.raw`(?=\{)`,
  patterns: [comments, typeExpression],
};

const returnStatement: Rule = {
  key: "return-statement",
  scope: "meta.statement.return.baml",
  begin: String.raw`^\s*(return)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.flow.return.baml" },
  },
  end: String.raw`(?=$|;)`,
  patterns: [comments, expression],
};

const breakStatement: Rule = {
  key: "break-statement",
  scope: "meta.statement.break.baml",
  begin: String.raw`^\s*(break)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.flow.break.baml" },
  },
  end: String.raw`(?=$|;)`,
  patterns: [comments],
};

const continueStatement: Rule = {
  key: "continue-statement",
  scope: "meta.statement.continue.baml",
  begin: String.raw`^\s*(continue)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.flow.continue.baml" },
  },
  end: String.raw`(?=$|;)`,
  patterns: [comments],
};

const letStatement: Rule = {
  key: "let-statement",
  scope: "meta.statement.let.baml",
  begin: String.raw`^\s*(?=let\b)`,
  end: String.raw`(?=$|;)`,
  patterns: [
    comments,
    {
      key: "let-statement-pattern",
      scope: "meta.pattern.statement.baml",
      begin: String.raw`\G\s*`,
      end: String.raw`(?=\s*=)`,
      patterns: [comments, pattern],
    },
    {
      key: "let-statement-assignment-operator",
      scope: "keyword.operator.assignment.baml",
      match: String.raw`=`,
    },
    expression,
  ],
};

const configBlock: IncludeRule = {
  key: "config-block",
  patterns: [],
};

configBlock.patterns = [
  {
    key: "config-block-body",
    scope: "meta.config.block.baml",
    begin: String.raw`\{`,
    beginCaptures: {
      "0": { scope: "punctuation.definition.block.begin.baml" },
    },
    end: String.raw`\}`,
    endCaptures: {
      "0": { scope: "punctuation.definition.block.end.baml" },
    },
    patterns: [
      comments,
      {
        key: "config-field",
        scope: "meta.field.config.baml",
        begin: String.raw`^\s*(${IDENT})\b\s*(:)?`,
        beginCaptures: {
          "1": { scope: "variable.other.property.baml" },
          "2": { scope: "punctuation.separator.colon.baml" },
        },
        end: String.raw`(?=,|\r?\n|\})`,
        patterns: [comments, configBlock, expression],
      },
      {
        key: "config-comma",
        scope: "punctuation.separator.comma.baml",
        match: String.raw`,`,
      },
    ],
  },
];

const clientItem: Rule = {
  key: "client",
  scope: "meta.client.baml",
  begin: String.raw`^\s*(client)\b(?:\s*(<)\s*(${IDENT})\s*(>)\s*|\s+)(${IDENT})\b(?=\s*\{)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.client.baml" },
    "2": { scope: "punctuation.definition.type-parameters.begin.baml" },
    "3": { scope: "support.type.client.baml" },
    "4": { scope: "punctuation.definition.type-parameters.end.baml" },
    "5": { scope: "entity.name.client.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [comments, configBlock],
};

const retryPolicyItem: Rule = {
  key: "retry-policy",
  scope: "meta.retry-policy.baml",
  begin: String.raw`^\s*(retry_policy)\s+(${IDENT})\b(?=\s*\{)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.retry-policy.baml" },
    "2": { scope: "entity.name.retry-policy.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [comments, configBlock],
};

const generatorItem: Rule = {
  key: "generator",
  scope: "meta.generator.baml",
  begin: String.raw`^\s*(generator)\b(?:\s+(${IDENT}))?\s*(?=\{)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.generator.baml" },
    "2": { scope: "entity.name.generator.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [comments, configBlock],
};

const typeAliasItem: Rule = {
  key: "type-alias",
  scope: "meta.type-alias.baml",
  begin: String.raw`^\s*(type)\s+(${IDENT})\s*(=)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.type-alias.baml" },
    "2": { scope: "entity.name.type.alias.baml" },
    "3": { scope: "keyword.operator.assignment.baml" },
  },
  end: String.raw`(?<=;)|(?=${TOP_LEVEL_ITEM_START})`,
  patterns: [comments, typeExpression, semicolon],
};

const testItem: IncludeRule = {
  key: "test",
  patterns: [],
};

const testsetItem: IncludeRule = {
  key: "testset",
  patterns: [],
};

const testHeader: Rule = {
  key: "test-header",
  scope: "meta.test.header.baml",
  begin: String.raw`\G\s*`,
  end: String.raw`(?=\{)`,
  patterns: [
    comments,
    {
      key: "test-with-keyword",
      scope: "keyword.operator.with.baml",
      match: String.raw`\bwith\b`,
    },
    conditionExpression,
  ],
};

testItem.patterns = [
  {
    key: "test-expression",
    scope: "meta.test.baml",
    begin: String.raw`^\s*(test)\b(?!(?:[^\S\r\n]+${IDENT}[^\S\r\n]*\{[^\S\r\n]*(?:functions|type_builder)\b))`,
    beginCaptures: {
      "1": { scope: "keyword.declaration.test.baml" },
    },
    end: String.raw`(?<=\})`,
    patterns: [
      comments,
      testHeader,
      codeBlock,
    ],
  },
];

testsetItem.patterns = [
  {
    key: "testset-expression",
    scope: "meta.testset.baml",
    begin: String.raw`^\s*(testset)\b`,
    beginCaptures: {
      "1": { scope: "keyword.declaration.testset.baml" },
    },
    end: String.raw`(?<=\})`,
    patterns: [
      comments,
      testHeader,
      {
        key: "testset-body",
        scope: "meta.testset.body.baml",
        begin: String.raw`\{`,
        beginCaptures: {
          "0": { scope: "punctuation.definition.block.begin.baml" },
        },
        end: String.raw`\}`,
        endCaptures: {
          "0": { scope: "punctuation.definition.block.end.baml" },
        },
        patterns: [comments, blockContents],
      },
    ],
  },
];

blockContents.patterns = [
  comments,
  testsetItem,
  testItem,
  letStatement,
  returnStatement,
  breakStatement,
  continueStatement,
  forStatement,
  whileStatement,
  expression,
  semicolon,
];

const functionBlock: Rule = {
  key: "function-block",
  scope: "meta.block.function.baml",
  begin: String.raw`\{`,
  beginCaptures: {
    "0": { scope: "punctuation.definition.block.begin.baml" },
  },
  end: String.raw`\}`,
  endCaptures: {
    "0": { scope: "punctuation.definition.block.end.baml" },
  },
  patterns: [
    {
      key: "llm-client-field",
      scope: "meta.field.llm.client.baml",
      begin: String.raw`^\s*(client)\b\s*(:)?(?!\s*\.)`,
      beginCaptures: {
        "1": { scope: "keyword.other.llm.client.baml" },
        "2": { scope: "punctuation.separator.colon.baml" },
      },
      end: String.raw`$`,
      patterns: [comments, expression],
    },
    {
      key: "llm-prompt-field",
      scope: "meta.field.llm.prompt.baml",
      begin: String.raw`^\s*(prompt)\b\s*(:)?`,
      beginCaptures: {
        "1": { scope: "keyword.other.llm.prompt.baml" },
        "2": { scope: "punctuation.separator.colon.baml" },
      },
      end: String.raw`$`,
      patterns: [comments, expression],
    },
    blockContents,
  ],
};

const functionItem: Rule = {
  key: "function",
  scope: "meta.function.baml",
  begin: String.raw`^\s*(function)\s+(${IDENT})\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.function.baml" },
    "2": { scope: "entity.name.function.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [comments, functionParameters, functionReturnType, functionBlock],
};

const classItem: Rule = {
  key: "class",
  scope: "meta.class.baml",
  begin: String.raw`\b(class)\s+(${IDENT})\s*(\{)`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.class.baml" },
    "2": { scope: "entity.name.type.class.baml" },
    "3": { scope: "punctuation.definition.block.begin.baml" },
  },
  end: String.raw`\}`,
  endCaptures: {
    "0": { scope: "punctuation.definition.block.end.baml" },
  },
  patterns: [comments, functionItem, field], // TODO: attributes
};

export const baml: Grammar = {
  $schema: tm.schema,
  name: "baml",
  scopeName: "source.baml",
  fileTypes: ["baml"],
  patterns: [comments, clientItem, retryPolicyItem, generatorItem, typeAliasItem, classItem, functionItem, testsetItem, testItem] satisfies Rule[],
};
