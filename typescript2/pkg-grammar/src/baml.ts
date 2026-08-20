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
const TYPE_ARGS_BEFORE_BLOCK = String.raw`(?:<[^{}=;\r\n]*>\s*)?`;
const BINDING_INTRO = String.raw`(?:let|const)`;

// Capturing path regexes shared by the type/value reference rules. DOTTED_REF
// has 4 groups (optional leading segment, its dot, the middle prefix, the final
// name); DOTTED_PATH has 2 (head, dotted tail).
const DOTTED_REF = String.raw`\b(?:(${IDENT})\s*(\.)\s*)?((?:${IDENT}${ACCESSOR})*)(${IDENT})\b`;
const DOTTED_PATH = String.raw`\b(${IDENT})((?:${ACCESSOR}${IDENT})*)\b`;

// --- Builtins --------------------------------------------------------------
//
// Reserved names, kept as arrays so they are a single source of truth shared by
// the rules below (and reusable elsewhere).

// Primitive types. Highlighted as builtin types, distinct from user types.
const BUILTIN_TYPES = [
  "int",
  "float",
  "bigint",
  "string",
  "bool",
  "image",
  "audio",
  "map",
  "json",
  "unknown",
  "never",
  "Self",
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
  "template_string",
  "class",
  "enum",
  "interface",
  "implements",
  "implement",
  "function",
  "testset",
  "test",
  "type",
];

const TOP_LEVEL_ITEM_START = String.raw`^\s*${oneOf(TOP_LEVEL_ITEMS)}\b`;
const STATEMENT_START = String.raw`(?:^\s*|\G\s*|(?<=[{;}])\s*)`;

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

// --- Shared leaf rules & block helpers --------------------------------------
//
// Single source of truth for the punctuation/operator leaves and the block
// shapes that recur across dozens of rules. Each shared rule object emits one
// repository entry, referenced wherever it appears.

const comma: Rule = {
  key: "comma",
  scope: "punctuation.separator.comma.baml",
  match: String.raw`,`,
};

const colonSeparator: Rule = {
  key: "colon-separator",
  scope: "punctuation.separator.colon.baml",
  match: String.raw`:`,
};

const accessorDot: Rule = {
  key: "accessor-dot",
  scope: "punctuation.accessor.baml",
  match: String.raw`\.`,
};

const assignmentOperator: Rule = {
  key: "assignment-operator",
  scope: "keyword.operator.assignment.baml",
  match: String.raw`=`,
};

const expressionIdentifier: Rule = {
  key: "expression-identifier",
  scope: "variable.other.readwrite.baml",
  match: String.raw`\b${IDENT}\b`,
};

// `{ "0": { scope } }` is the begin/endCaptures shape used by most delimited
// rules; caps0 names it once. Only valid where group 0 is the sole capture.
const caps0 = (scope: BamlScope) => ({ "0": { scope } });

const BLOCK_BEGIN = "punctuation.definition.block.begin.baml";
const BLOCK_END = "punctuation.definition.block.end.baml";

// A `{ ... }` block carrying the standard block-punctuation scopes. `end`
// defaults to a bare `}`; the blocks that also bail at the next top-level item
// pass their own.
function braceBlock(
  key: string,
  scope: BamlScope,
  patterns: Rule[],
  end: string = String.raw`\}`,
): Rule {
  return {
    key,
    scope,
    begin: String.raw`\{`,
    beginCaptures: caps0(BLOCK_BEGIN),
    end,
    endCaptures: caps0(BLOCK_END),
    patterns,
  };
}

// Repeated end-pattern fragments, named once (mirrors loopStatementEnd below).
const EXPRESSION_BODY_END = String.raw`(?<=\})|(?=,|;|$)`;
const MEMBER_SIGNATURE_END = String.raw`(?<=;)|(?=\r?\n|\})`;
// Also stop at `}`: a statement that is the last in an inline block (e.g.
// `{ return "hello" }`) has no `;`/newline before the block close, and must not
// swallow it. A `}` inside the statement's own value is consumed by a sub-rule
// (codeBlock/constructor/...) first, so this only ever matches the block close.
const STATEMENT_END = String.raw`(?=[;}]|$)`;
const ITEM_BODY_END = String.raw`(?<=\})|(?=${TOP_LEVEL_ITEM_START})`;
const HEADER_END = String.raw`(?=\{)|(?=${TOP_LEVEL_ITEM_START})`;

// Trailing alternation that ends an attribute / enum-variant at the next item.
const ITEM_END_TAIL = String.raw`\s*\}|\s+${IDENT}\b|\r?\n\s*(?:${IDENT}\b|@@|\})`;

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

const standardControlEscape: Rule = {
  key: "string-escape-control",
  scope: "constant.character.escape.control.baml",
  match: String.raw`\\[ntr0bvf]`,
};

const quotedDelimiterEscape: Rule = {
  key: "string-escape-quoted-delimiter",
  scope: "constant.character.escape.delimiter.baml",
  match: String.raw`\\["\\]`,
};

const unknownStringEscape: Rule = {
  key: "string-escape-unknown",
  scope: "constant.character.escape.unknown.baml",
  match: String.raw`\\.`,
};

// Quoted strings decode the standard control escapes plus `\\` and `\"`.
// Unknown escapes are preserved by the lowerer, so scope them distinctly rather
// than marking them invalid.
const stringEscape: Rule = {
  key: "string-escape",
  patterns: [standardControlEscape, quotedDelimiterEscape, unknownStringEscape],
};

const byteStringControlEscape: Rule = {
  key: "byte-string-escape-control",
  scope: "constant.character.escape.control.baml",
  match: String.raw`\\[ntr0]`,
};

const byteStringHexEscape: Rule = {
  key: "byte-string-escape-hex",
  scope: "constant.character.escape.hex.baml",
  match: String.raw`\\x[0-9A-Fa-f]{2}`,
};

const byteStringInvalidHexEscape: Rule = {
  key: "byte-string-escape-invalid-hex",
  scope: "invalid.illegal.escape.hex.baml",
  match: String.raw`\\x(?:[0-9A-Fa-f]{0,1}(?=["\\]|$)|[0-9A-Fa-f]?[^0-9A-Fa-f"\\])`,
};

const byteStringInvalidEscape: Rule = {
  key: "byte-string-escape-invalid",
  scope: "invalid.illegal.escape.baml",
  match: String.raw`\\.`,
};

// Byte strings decode `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, and `\xHH`.
// Everything else is a lowering error.
const byteStringEscape: Rule = {
  key: "byte-string-escape",
  patterns: [
    byteStringHexEscape,
    byteStringControlEscape,
    quotedDelimiterEscape,
    byteStringInvalidHexEscape,
    byteStringInvalidEscape,
  ],
};

const backtickDelimiterEscape: Rule = {
  key: "backtick-string-escape-delimiter",
  scope: "constant.character.escape.delimiter.baml",
  match: String.raw`\\[` + "`" + String.raw`$]`,
};

// Backtick strings share normal string escapes and add \` plus \$ for literal
// backticks and literal interpolation starts.
const backtickStringEscape: Rule = {
  key: "backtick-string-escape",
  patterns: [
    standardControlEscape,
    quotedDelimiterEscape,
    backtickDelimiterEscape,
    unknownStringEscape,
  ],
};

const backtickInterpolation: IncludeRule = {
  key: "backtick-interpolation",
  patterns: [],
};

// Plain double-quoted string: `"..."`.
const stringLiteral: Rule = {
  key: "string-literal",
  scope: "string.quoted.double.baml",
  begin: String.raw`"`,
  beginCaptures: caps0("punctuation.definition.string.begin.baml"),
  end: String.raw`"`,
  endCaptures: caps0("punctuation.definition.string.end.baml"),
  patterns: [stringEscape],
};

// Byte string literal: `b"..."`. The parser only treats the prefix as special
// when it is adjacent to the quote; `b "..."` remains an identifier plus string.
const byteStringLiteral: Rule = {
  key: "byte-string-literal",
  scope: "string.quoted.binary.double.baml",
  begin: String.raw`\b(b)(")`,
  beginCaptures: {
    "1": { scope: "storage.type.string.baml" },
    "2": { scope: "punctuation.definition.string.begin.baml" },
  },
  end: String.raw`"`,
  endCaptures: caps0("punctuation.definition.string.end.baml"),
  patterns: [byteStringEscape],
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
// lone `` ` ``) is just string content.
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
    patterns: [backtickInterpolation, backtickStringEscape],
  }));
}

const backtickStringLiteral: Rule = {
  key: "backtick-string",
  patterns: backtickStringRules(MAX_DELIMITER),
};

// Type literals are also valid value literals; byte strings are value-only.
const typeLiteral: Rule = {
  key: "type-literal",
  patterns: [
    booleanLiteral,
    nullLiteral,
    numericLiteral,
    stringLiteral,
    rawStringLiteral,
    backtickStringLiteral,
  ],
};

const literal: Rule = {
  key: "literal",
  patterns: [byteStringLiteral, typeLiteral],
};

const expression: IncludeRule = {
  key: "expression",
  patterns: [],
};

const parenthesizedExpression: Rule = {
  key: "parenthesized-expression",
  scope: "meta.group.expression.baml",
  begin: String.raw`\(`,
  beginCaptures: caps0("punctuation.definition.group.begin.baml"),
  end: String.raw`\)`,
  endCaptures: caps0("punctuation.definition.group.end.baml"),
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

const codeBlock: Rule = braceBlock("code-block", "meta.block.code.baml", [comments, blockContents]);

const templateComment: Rule = {
  key: "template-comment",
  scope: "comment.block.template.baml",
  begin: String.raw`\{#`,
  beginCaptures: caps0("punctuation.definition.comment.begin.baml"),
  end: String.raw`#\}`,
  endCaptures: caps0("punctuation.definition.comment.end.baml"),
};

const TEMPLATE_KEYWORDS = [
  "for",
  "endfor",
  "if",
  "elif",
  "else",
  "endif",
  "in",
  "set",
  "filter",
  "endfilter",
  "macro",
  "endmacro",
  "raw",
  "endraw",
];

const templateKeyword: Rule = {
  key: "template-keyword",
  scope: "keyword.control.template.baml",
  match: String.raw`\b${oneOf(TEMPLATE_KEYWORDS)}\b`,
};

const templateInterpolation: Rule = {
  key: "template-interpolation",
  scope: "meta.template.interpolation.baml",
  begin: String.raw`\{\{`,
  beginCaptures: caps0("punctuation.section.interpolation.begin.baml"),
  end: String.raw`\}\}`,
  endCaptures: caps0("punctuation.section.interpolation.end.baml"),
  patterns: [comments, templateKeyword, expression],
};

const templateControl: Rule = {
  key: "template-control",
  scope: "meta.template.control.baml",
  begin: String.raw`\{%`,
  beginCaptures: caps0("punctuation.section.template.begin.baml"),
  end: String.raw`%\}`,
  endCaptures: caps0("punctuation.section.template.end.baml"),
  patterns: [comments, templateKeyword, expression],
};

const templateStringBodyPatterns = [
  templateComment,
  templateControl,
  templateInterpolation,
] satisfies Rule[];

const templateQuotedStringBody: Rule = {
  key: "template-quoted-string-body",
  scope: "string.quoted.double.template.baml",
  begin: String.raw`"`,
  beginCaptures: caps0("punctuation.definition.string.begin.baml"),
  end: String.raw`"`,
  endCaptures: caps0("punctuation.definition.string.end.baml"),
  patterns: [...templateStringBodyPatterns, stringEscape],
};

function templateRawStringBodyRules(max: number): Rule[] {
  return Array.from({ length: max }, (_, i) => max - i).map((n) => ({
    key: `template-raw-string-body-${n}`,
    scope: "string.quoted.raw.template.baml",
    begin: String.raw`(?<!#)(#{${n}})(")`,
    beginCaptures: {
      "1": { scope: "punctuation.definition.string.begin.baml" },
      "2": { scope: "punctuation.definition.string.begin.baml" },
    },
    end: String.raw`(")(#{${n}})`,
    endCaptures: {
      "1": { scope: "punctuation.definition.string.end.baml" },
      "2": { scope: "punctuation.definition.string.end.baml" },
    },
    patterns: templateStringBodyPatterns,
  }));
}

const templateStringBody: Rule = {
  key: "template-string-body",
  patterns: [
    ...templateRawStringBodyRules(MAX_DELIMITER),
    templateQuotedStringBody,
  ],
};

const arrayExpression: Rule = {
  key: "array-expression",
  scope: "meta.expression.array.baml",
  begin: String.raw`\[`,
  beginCaptures: caps0("punctuation.definition.array.begin.baml"),
  end: String.raw`\]`,
  endCaptures: caps0("punctuation.definition.array.end.baml"),
  patterns: [
    comments,
    expression,
    comma,
  ],
};

const mapExpression: Rule = {
  key: "map-expression",
  scope: "meta.expression.map.baml",
  begin: String.raw`\{(?=\s*(?:\}|["#]|${DOTTED_IDENT}\s*:))`,
  beginCaptures: caps0("punctuation.definition.map.begin.baml"),
  end: String.raw`\}`,
  endCaptures: caps0("punctuation.definition.map.end.baml"),
  patterns: [
    comments,
    {
      key: "map-entry",
      scope: "meta.map.entry.baml",
      begin: String.raw`\s*(?=(?:"|#|${DOTTED_IDENT}\s*:))`,
      end: String.raw`(?=,|\})`,
      patterns: [
        comments,
        {
          key: "map-entry-key",
          scope: tm.meta,
          match: DOTTED_PATH + String.raw`(?=\s*:)`,
          captures: {
            "1": { scope: "variable.other.property.baml" },
            "2": {
              patterns: [
                accessorDot,
                {
                  key: "map-entry-key-segment",
                  scope: "variable.other.property.baml",
                  match: String.raw`\b${IDENT}\b`,
                },
              ],
            },
          },
        },
        stringLiteral,
        rawStringLiteral,
        colonSeparator,
        expression,
      ],
    },
    comma,
  ],
};

function typeArgumentsRule(key: string, endLookahead: string): Rule {
  return {
    key,
    scope: "meta.type-arguments.baml",
    begin: String.raw`<`,
    beginCaptures: caps0("punctuation.definition.type-arguments.begin.baml"),
    end: String.raw`>` + endLookahead,
    endCaptures: caps0("punctuation.definition.type-arguments.end.baml"),
    patterns: [comments, typeExpression],
  };
}

const constructorExpression: Rule = {
  key: "constructor-expression",
  scope: "meta.constructor.expression.baml",
  begin: String.raw`\b${DOTTED_IDENT}\b(?=\s*${TYPE_ARGS_BEFORE_BLOCK}\{)`,
  beginCaptures: {
    "0": {
      patterns: [
        accessorDot,
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
    comments,
    typeArgumentsRule("constructor-type-arguments", String.raw`(?=\s*\{)`),
    {
      key: "constructor-body",
      scope: "meta.constructor.body.baml",
      begin: String.raw`\{`,
      beginCaptures: caps0("punctuation.definition.constructor.body.begin.baml"),
      end: String.raw`\}`,
      endCaptures: caps0("punctuation.definition.constructor.body.end.baml"),
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
          // Shorthand (colon-less) field form used in test `args` and config
          // blocks: `text "Jane"`, `functions [F]`, `max_retries 3`. The name
          // is the identifier at entry start (right after `{`, `,`, a
          // newline, or a closing block comment); the lookahead pins a
          // literal-ish value start so a bare expression entry is never
          // misread as a field name.
          key: "constructor-field-shorthand",
          scope: "meta.constructor.field.baml",
          begin: String.raw`(?:^|(?<=[{,])|(?<=\*/))[ \t]*(${IDENT})[ \t]+(?=#+"|"|b"|\`|\[|[0-9+-]|true\b|false\b|null\b|env\b)`,
          beginCaptures: {
            "1": { scope: "variable.other.property.baml" },
          },
          end: String.raw`(?=,|\})`,
          patterns: [comments, expression],
        },
        comma,
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




const expressionPathRootCapture = {
  patterns: [
    namespaceRoot,
    environmentExpressionRoot,
    selfExpressionRoot,
    expressionIdentifier,
  ],
};

const expressionPathMemberCapture = {
  patterns: [accessorDot, expressionIdentifier],
};

// Every call-style expression shares one parenthesized argument list.
const callArguments: Rule = {
  key: "call-arguments",
  scope: "meta.function-call.arguments.baml",
  begin: String.raw`\(`,
  beginCaptures: caps0("punctuation.definition.arguments.begin.baml"),
  end: String.raw`\)`,
  endCaptures: caps0("punctuation.definition.arguments.end.baml"),
  patterns: [comments, expression, comma],
};

// The lookahead every call begin uses to require `(` (optionally after a
// type-argument list) without consuming it.
const CALL_ARGS_LOOKAHEAD = String.raw`(?=\s*(?:<[^(){};]*>\s*)?\()`;

// `obj.method(...)` and `obj?.method(...)` differ only by the accessor.
function memberCall(
  key: string,
  scope: BamlScope,
  accessor: string,
  typeArgsKey: string,
): Rule {
  return {
    key,
    scope,
    begin: accessor + String.raw`\s*(${IDENT})\b` + CALL_ARGS_LOOKAHEAD,
    beginCaptures: {
      "1": { scope: "punctuation.accessor.baml" },
      "2": { scope: "entity.name.function.baml" },
    },
    end: String.raw`(?<=\))`,
    patterns: [
      comments,
      typeArgumentsRule(typeArgsKey, String.raw`(?=\s*\()`),
      callArguments,
    ],
  };
}

// `obj.field` and `obj?.field` differ only by the accessor.
function memberAccess(key: string, scope: BamlScope, accessor: string): Rule {
  return {
    key,
    scope,
    match: accessor + String.raw`\s*(${IDENT})\b`,
    captures: {
      "1": { scope: "punctuation.accessor.baml" },
      "2": { scope: "variable.other.readwrite.baml" },
    },
  };
}

const functionCallExpression: Rule = {
  key: "function-call-expression",
  scope: "meta.function-call.baml",
  begin: DOTTED_REF + CALL_ARGS_LOOKAHEAD,
  beginCaptures: {
    "1": expressionPathRootCapture,
    "2": { scope: "punctuation.accessor.baml" },
    "3": expressionPathMemberCapture,
    "4": { scope: "entity.name.function.baml" },
  },
  end: String.raw`(?<=\))`,
  patterns: [
    comments,
    typeArgumentsRule("function-call-type-arguments", String.raw`(?=\s*\()`),
    callArguments,
  ],
};

const optionalCallExpression: Rule = {
  key: "optional-call-expression",
  scope: "meta.function-call.optional.baml",
  begin: String.raw`(\?\.)\s*(?=\()`,
  beginCaptures: {
    "1": { scope: "punctuation.accessor.baml" },
  },
  end: String.raw`(?<=\))`,
  patterns: [
    comments,
    callArguments,
  ],
};

const optionalIndexExpression: Rule = {
  key: "optional-index-expression",
  scope: "meta.index.optional.baml",
  begin: String.raw`(\?\.)\s*(?=\[)`,
  beginCaptures: {
    "1": { scope: "punctuation.accessor.baml" },
  },
  end: String.raw`(?<=\])`,
  patterns: [
    comments,
    {
      key: "optional-index-arguments",
      scope: "meta.index.arguments.baml",
      begin: String.raw`\[`,
      beginCaptures: caps0("punctuation.definition.bracket.begin.baml"),
      end: String.raw`\]`,
      endCaptures: caps0("punctuation.definition.bracket.end.baml"),
      patterns: [comments, expression],
    },
  ],
};

const optionalMethodCallExpression: Rule = memberCall(
  "optional-method-call-expression",
  "meta.function-call.optional.member.baml",
  String.raw`(\?\.)`,
  "optional-method-call-type-arguments",
);

const optionalFieldAccessExpression: Rule = memberAccess(
  "optional-field-access-expression",
  "meta.field-access.optional.baml",
  String.raw`(\?\.)`,
);

const postfixMethodCallExpression: Rule = memberCall(
  "postfix-method-call-expression",
  "meta.function-call.member.baml",
  String.raw`([.$])`,
  "postfix-method-call-type-arguments",
);

const fieldAccessExpression: Rule = memberAccess(
  "field-access-expression",
  "meta.field-access.baml",
  String.raw`([.$])`,
);

const dottedExpression: Rule = {
  key: "dotted-expression",
  scope: tm.meta,
  // Like DOTTED_PATH, but a path segment never absorbs an `.as<` projection, so
  // `self.as<T>` leaves `.as<T>` for upcastExpression instead of reading `as`
  // as a member and `<`/`>` as comparisons.
  match: String.raw`\b(${IDENT})((?:${ACCESSOR}(?!as\b\s*<)${IDENT})*)\b`,
  captures: {
    "1": expressionPathRootCapture,
    "2": expressionPathMemberCapture,
  },
};

// `base.as<Interface>` is a static interface projection / upcast -- a dedicated
// expression form, not a method call: `as` is a contextual keyword and the
// angle brackets hold the target type. Matched only when a `<` follows, so plain
// `.as` field access and `.as(...)` calls are unaffected.
const upcastExpression: Rule = {
  key: "upcast-expression",
  scope: "meta.expression.upcast.baml",
  begin: String.raw`(\.)\s*(as)\b(?=\s*<)`,
  beginCaptures: {
    "1": { scope: "punctuation.accessor.baml" },
    "2": { scope: "keyword.operator.as.baml" },
  },
  end: String.raw`(?<=>)`,
  patterns: [comments, typeArgumentsRule("upcast-type-arguments", "")],
};

const spawnExpression: Rule = {
  key: "spawn-expression",
  scope: "meta.expression.spawn.baml",
  begin: String.raw`\b(spawn)\b`,
  beginCaptures: {
    "1": { scope: "keyword.operator.spawn.baml" },
  },
  end: EXPRESSION_BODY_END,
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
          key: "spawn-name",
          scope: "meta.spawn.name.baml",
          begin: String.raw`\G\s*(?!with\b)(?=[^\{\r\n])`,
          end: String.raw`(?=\s+with\b|\s*\{)`,
          patterns: [comments, conditionExpression],
        },
        {
          key: "spawn-with-clause",
          scope: "meta.spawn.options.baml",
          begin: String.raw`\b(with)\b`,
          beginCaptures: {
            "1": { scope: "keyword.operator.with.baml" },
          },
          end: String.raw`(?=\{)`,
          patterns: [
            comments,
            conditionExpression,
            comma,
          ],
        },
      ],
    },
    codeBlock,
  ],
};

const awaitExpression: Rule = {
  key: "await-expression",
  scope: "keyword.operator.await.baml",
  match: String.raw`\bawait\b`,
};

const throwExpression: Rule = {
  key: "throw-expression",
  scope: "keyword.control.flow.throw.baml",
  match: String.raw`\bthrow\b`,
};

const catchExpression: Rule = {
  key: "catch-expression",
  scope: "meta.expression.catch.baml",
  begin: String.raw`\b(catch|catch_all)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.exception.catch.baml" },
  },
  end: EXPRESSION_BODY_END,
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
    assignmentOperator,
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


// A type reference, namespaced or not: `Foo`, or `name.space.Foo`. One rule
// covers both. The first namespace segment is captured on its own (group 1) so
// the builtin roots `root` / `baml` can be coloured only in leading position;
// group 2 is its separator, group 3 the remaining `space.` prefix, and group 4
// the final type name. For a bare `Foo`, only group 4 is present.
const typeReference: Rule = {
  key: "type-reference",
  // The whole reference gets no single colour; only its captures do.
  scope: tm.meta,
  match: DOTTED_REF,
  captures: {
    "1": { patterns: [namespaceRoot, namespaceSegment] },
    "2": { scope: "punctuation.accessor.baml" },
    "3": { patterns: [namespaceSegment, accessorDot] },
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

const typeAsOperator: Rule = {
  key: "type-as-operator",
  scope: "keyword.operator.as.baml",
  match: String.raw`\bas\b`,
};


const associatedTypeProjection: Rule = {
  key: "associated-type-projection",
  scope: "meta.type.associated-projection.baml",
  begin: String.raw`\((?=[^)\r\n]*\bas\b)`,
  beginCaptures: caps0("punctuation.definition.type.begin.baml"),
  end: String.raw`(\))\s*(\.)\s*(${IDENT})\b`,
  endCaptures: {
    "1": { scope: "punctuation.definition.type.end.baml" },
    "2": { scope: "punctuation.accessor.baml" },
    "3": { scope: "entity.name.type.associated.baml" },
  },
  patterns: [comments, typeAsOperator, typeExpression],
};

// Brackets, angles, and commas are pure punctuation.
const typePunctuation: Rule = {
  key: "type-punctuation",
  scope: "punctuation.definition.type.baml",
  match: String.raw`[\[\]<>,()]`,
};


const semicolon: Rule = {
  key: "semicolon",
  scope: "punctuation.terminator.statement.baml",
  match: String.raw`;`,
};


const attributePathSegment: Rule = {
  key: "attribute-path-segment",
  scope: "storage.type.annotation.baml",
  match: String.raw`\b${IDENT}\b`,
};

const DOTTED_ATTRIBUTE_IDENT = String.raw`${IDENT}${ACCESSOR}${IDENT}(?:${ACCESSOR}${IDENT})*`;

const attributeExpressionBlock: Rule = {
  key: "attribute-expression-block",
  scope: "meta.attribute.expression.baml",
  begin: String.raw`\{\{`,
  beginCaptures: caps0("punctuation.section.expression.begin.baml"),
  end: String.raw`\}\}`,
  endCaptures: caps0("punctuation.section.expression.end.baml"),
  patterns: [comments, expression],
};

const attributeArguments: Rule = {
  key: "attribute-arguments",
  scope: "meta.attribute.arguments.baml",
  begin: String.raw`\(`,
  beginCaptures: caps0("punctuation.definition.annotation-arguments.begin.bracket.round.baml"),
  end: String.raw`\)`,
  endCaptures: caps0("punctuation.definition.annotation-arguments.end.bracket.round.baml"),
  patterns: [
    comments,
    attributeExpressionBlock,
    literal,
    {
      key: "attribute-unquoted-string",
      scope: "string.unquoted.baml",
      match: String.raw`\b${IDENT}\b`,
    },
    comma,
  ],
};

// `@name` / `@@name` annotation heads tokenize their dotted path identically.
const attributeNameCaptures = {
  "1": {
    scope: "punctuation.definition.annotation.baml storage.type.annotation.baml",
  },
  "2": { patterns: [accessorDot, attributePathSegment] },
};

const attribute: Rule = {
  key: "attribute",
  scope: "meta.attribute.baml",
  begin: String.raw`(@)(?!@)\s*(${DOTTED_IDENT})\b`,
  beginCaptures: attributeNameCaptures,
  end: String.raw`(?<=\))|(?=\s*@|\s*,|${ITEM_END_TAIL})`,
  patterns: [comments, attributeArguments],
};

const bareAttribute: Rule = {
  key: "bare-attribute",
  scope: "meta.attribute.baml",
  match: String.raw`(@)(?!@)\s*(${DOTTED_ATTRIBUTE_IDENT}|skip)\b(?=\s*(?:@|,|\}|$|//|/\*))`,
  captures: attributeNameCaptures,
};

const blockAttribute: Rule = {
  key: "block-attribute",
  scope: "meta.attribute.block.baml",
  begin: String.raw`(@@)\s*(${DOTTED_IDENT})\b`,
  beginCaptures: attributeNameCaptures,
  end: String.raw`(?<=\))|(?=\s*@@|\s*@|\s*,|${ITEM_END_TAIL})`,
  patterns: [comments, attributeArguments],
};

// A field's type. Order matters: primitives and literals are claimed before the
// catch-all `typeReference`, so `string` is a primitive and `true` / `"active"`
// / `42` are literal types rather than bare identifiers.
typeExpression.patterns = [
  comments,
  primitiveType,
  typeLiteral,
  typeArrowOperator,
  typeThrowsOperator,
  associatedTypeProjection,
  typeReference,
  bareAttribute,
  attribute,
  typeAsOperator,
  assignmentOperator,
  optionalOperator,
  unionOperator,
  typePunctuation,
  colonSeparator,
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
  match: String.raw`\b(${BINDING_INTRO})\s+(_)\b`,
  captures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "variable.language.wildcard.baml" },
  },
};

const typedBindingPattern: Rule = {
  key: "typed-binding-pattern",
  scope: "meta.pattern.binding.baml",
  match: String.raw`\b(${BINDING_INTRO})\s+(${IDENT})\b\s*(:)`,
  captures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "variable.other.binding.baml" },
    "3": { scope: "punctuation.separator.colon.baml" },
  },
};

const bareBindingPattern: Rule = {
  key: "bare-binding-pattern",
  scope: "meta.pattern.binding.baml",
  match: String.raw`\b(${BINDING_INTRO})\s+(${IDENT})\b`,
  captures: {
    "1": { scope: "keyword.declaration.binding.baml" },
    "2": { scope: "variable.other.binding.baml" },
  },
};

const classDestructurePattern: Rule = {
  key: "class-destructure-pattern",
  scope: "meta.pattern.destructure.class.baml",
  begin: String.raw`\b(?:(${BINDING_INTRO})\s+)?(${DOTTED_IDENT})\b\s*(?=${TYPE_ARGS_BEFORE_BLOCK}\{)`,
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
      beginCaptures: caps0("punctuation.definition.pattern.destructure.begin.baml"),
      end: String.raw`\}`,
      endCaptures: caps0("punctuation.definition.pattern.destructure.end.baml"),
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
        comma,
      ],
    },
  ],
};

const arrayDestructurePattern: Rule = {
  key: "array-destructure-pattern",
  scope: "meta.pattern.destructure.array.baml",
  begin: String.raw`(?:\b(${BINDING_INTRO})\b\s*)?(\[)`,
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
    comma,
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

// `is <pattern>` ends before the next operator/delimiter; `extra` appends to the
// end char class so the condition-position variant also stops at `{`.
function isPatternRule(key: string, inner: Rule, extra = ""): Rule {
  return {
    key,
    scope: "meta.expression.is.baml",
    begin: String.raw`\b(is)\b`,
    beginCaptures: { "1": { scope: "keyword.operator.is.baml" } },
    end: String.raw`(?=\s*(?:&&|\|\||=>|[,);\]}${extra}]|$))`,
    patterns: [comments, inner],
  };
}

const isPatternExpression: Rule = isPatternRule("is-pattern-expression", pattern);

// In condition position (`if` / `while` / `else if`) a `{` opens the block, so
// `if r is Empty { ... }` must read `{` as the block, not as a class-destructure
// `Empty { ... }` that swallows it. Mirror the parser's Rust-style restriction:
// drop classDestructurePattern from the `is` pattern and stop the `is`
// expression at `{`. (conditionExpression already drops constructorExpression
// for the same `X { }`-vs-block ambiguity.)
const conditionPattern: IncludeRule = {
  key: "condition-pattern",
  patterns: [],
};
conditionPattern.patterns = pattern.patterns.filter(
  (rule) => rule !== classDestructurePattern,
);

const conditionIsPatternExpression: Rule = isPatternRule(
  "condition-is-pattern-expression",
  conditionPattern,
  "{",
);

// `let`/`const` binding pattern that ends at the `=`. requireIntro gates the
// lookahead for the contexts that must see a binding keyword first.
function bindingPatternRule(key: string, scope: BamlScope, requireIntro: boolean): Rule {
  return {
    key,
    scope,
    begin: requireIntro ? String.raw`\G\s*(?=${BINDING_INTRO}\b)` : String.raw`\G\s*`,
    end: String.raw`(?=\s*=)`,
    patterns: [comments, pattern],
  };
}

const ifExpressionEnd = String.raw`(?!\s*else\b)(?:(?<=\})(?=\s*(?:[,);]|$))|(?=,|\)|;|$))`;

function ifConditionPattern(
  key: string,
  scope: BamlScope,
  end: string,
  avoidElse = false,
): Rule {
  return {
    key,
    scope,
    begin: avoidElse ? String.raw`\G(?!\s*else\b)\s*` : String.raw`\G\s*`,
    end,
    patterns: [comments, conditionExpression],
  };
}

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

const elseClause: Rule = {
  key: "if-else-clause",
  scope: "meta.else.baml",
  begin: String.raw`${STATEMENT_START}(else)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.conditional.baml" },
  },
  end: ifExpressionEnd,
  patterns: [comments, ifExpression, codeBlock],
};

ifExpression.patterns = [
  comments,
  bindingPatternRule("if-let-pattern", "meta.pattern.if-let.baml", true),
  assignmentOperator,
  ifConditionPattern("if-condition", "meta.if.condition.baml", String.raw`(?=\{)`, true),
  codeBlock,
  elseClause,
];

const matchScrutineeGroup: Rule = {
  key: "match-scrutinee-group",
  scope: "meta.match.scrutinee.group.baml",
  begin: String.raw`\(`,
  beginCaptures: caps0("punctuation.definition.group.begin.baml"),
  end: String.raw`\)`,
  endCaptures: caps0("punctuation.definition.group.end.baml"),
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
    expression,
  ],
};

catchExpression.patterns = [
  comments,
  {
    key: "catch-binding-list",
    scope: "meta.catch.binding-list.baml",
    begin: String.raw`\(`,
    beginCaptures: caps0("punctuation.definition.catch-binding.begin.baml"),
    end: String.raw`\)`,
    endCaptures: caps0("punctuation.definition.catch-binding.end.baml"),
    patterns: [
      comments,
      {
        key: "catch-binding",
        scope: "variable.parameter.catch.baml",
        match: String.raw`\b${IDENT}\b`,
      },
      comma,
    ],
  },
  braceBlock("catch-block", "meta.block.catch.baml", [comments, matchArm, comma]),
];

const matchBlock: Rule = braceBlock("match-block", "meta.block.match.baml", [
  comments,
  matchArm,
  comma,
]);

const matchExpression: Rule = {
  key: "match-expression",
  scope: "meta.expression.match.baml",
  begin: String.raw`\b(match)\b`,
  beginCaptures: {
    "1": { scope: "keyword.control.match.baml" },
  },
  end: EXPRESSION_BODY_END,
  patterns: [comments, matchScrutineeGroup, matchBlock, matchScrutinee],
};

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
    bindingPatternRule("while-let-pattern", "meta.pattern.while-let.baml", true),
    assignmentOperator,
    ifConditionPattern("while-condition", "meta.while.condition.baml", String.raw`(?=\{)`),
    codeBlock,
  ],
};

const letElseClause: Rule = {
  key: "let-else-clause",
  scope: "meta.statement.let.else.baml",
  begin: String.raw`\b(else)\b(?=\s*\{)`,
  beginCaptures: {
    "1": { scope: "keyword.control.conditional.else.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [comments, codeBlock],
};

// Shared across both for-header variants (and both forHeaderPatterns calls);
// the in-pattern begin is context-independent so one object serves every site.
const forInPattern: Rule = {
  key: "for-in-pattern",
  scope: "meta.pattern.for-in.baml",
  begin: String.raw`(?=${BINDING_INTRO}\b)`,
  end: String.raw`(?=\s+in\b)`,
  patterns: [comments, pattern],
};

const forInKeyword: Rule = {
  key: "for-in-keyword",
  scope: "keyword.operator.in.baml",
  match: String.raw`\bin\b`,
};

function forHeaderPatterns(
  keyPrefix: string,
  parenthesizedEnd: string,
  unparenthesizedEnd: string,
): Rule[] {
  return [
    {
      key: `${keyPrefix}-parenthesized-in-header`,
      scope: "meta.for.header.baml",
      begin: String.raw`\G\s*(\()(?=\s*${BINDING_INTRO}\b(?:(?![=;]).)*\bin\b)`,
      beginCaptures: {
        "1": { scope: "punctuation.definition.for-header.begin.baml" },
      },
      end: parenthesizedEnd,
      endCaptures: caps0("punctuation.definition.for-header.end.baml"),
      patterns: [
        comments,
        forInPattern,
        forInKeyword,
        expression,
      ],
    },
    {
      key: `${keyPrefix}-parenthesized-c-style-header`,
      scope: "meta.for.header.baml",
      begin: String.raw`\G\s*(\()`,
      beginCaptures: {
        "1": { scope: "punctuation.definition.for-header.begin.baml" },
      },
      end: parenthesizedEnd,
      endCaptures: caps0("punctuation.definition.for-header.end.baml"),
      patterns: [
        comments,
        {
          key: `${keyPrefix}-c-style-let-initializer`,
          scope: "meta.for.initializer.baml",
          begin: String.raw`(?=${BINDING_INTRO}\b)`,
          end: String.raw`(?=;)`,
          patterns: [comments, pattern, letElseClause, expression],
        },
        semicolon,
        expression,
      ],
    },
    {
      key: `${keyPrefix}-unparenthesized-header`,
      scope: "meta.for.header.baml",
      begin: String.raw`\G\s*(?=${BINDING_INTRO}\b)`,
      end: unparenthesizedEnd,
      patterns: [
        comments,
        forInPattern,
        forInKeyword,
        conditionExpression,
      ],
    },
  ];
}

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
    ...forHeaderPatterns(
      "for",
      String.raw`\)(?=\s*\{)`,
      String.raw`(?=\{)`,
    ),
    codeBlock,
  ],
};

// A bare `${ keyword }` interpolation control (else / endfor / endif).
function interpolationKeyword(
  key: string,
  metaScope: BamlScope,
  keyword: string,
  kwScope: BamlScope,
): Rule {
  return {
    key,
    scope: metaScope,
    match: String.raw`(\$\{)\s*(${keyword})\s*(\})`,
    captures: {
      "1": { scope: "punctuation.section.interpolation.begin.baml" },
      "2": { scope: kwScope },
      "3": { scope: "punctuation.section.interpolation.end.baml" },
    },
  };
}

backtickInterpolation.patterns = [
  {
    key: "backtick-for-open",
    scope: "meta.interpolation.control.for.baml",
    begin: String.raw`(\$\{)\s*(for)\b`,
    beginCaptures: {
      "1": { scope: "punctuation.section.interpolation.begin.baml" },
      "2": { scope: "keyword.control.loop.for.baml" },
    },
    end: String.raw`\}`,
    endCaptures: caps0("punctuation.section.interpolation.end.baml"),
    patterns: [
      comments,
      ...forHeaderPatterns(
        "backtick-for",
        String.raw`\)(?=\s*\})`,
        String.raw`(?=\})`,
      ),
    ],
  },
  {
    key: "backtick-else-if",
    scope: "meta.interpolation.control.else-if.baml",
    begin: String.raw`(\$\{)\s*(else)\b\s*(if)\b`,
    beginCaptures: {
      "1": { scope: "punctuation.section.interpolation.begin.baml" },
      "2": { scope: "keyword.control.conditional.baml" },
      "3": { scope: "keyword.control.conditional.baml" },
    },
    end: String.raw`\}`,
    endCaptures: caps0("punctuation.section.interpolation.end.baml"),
    patterns: [
      comments,
      ifConditionPattern(
        "backtick-else-if-condition",
        "meta.if.condition.baml",
        String.raw`(?=\})`,
      ),
    ],
  },
  {
    key: "backtick-if-open",
    scope: "meta.interpolation.control.if.baml",
    begin: String.raw`(\$\{)\s*(if)\b(?=(?:(?![\{\}]).)*\})`,
    beginCaptures: {
      "1": { scope: "punctuation.section.interpolation.begin.baml" },
      "2": { scope: "keyword.control.conditional.baml" },
    },
    end: String.raw`\}`,
    endCaptures: caps0("punctuation.section.interpolation.end.baml"),
    patterns: [
      comments,
      ifConditionPattern(
        "backtick-if-condition",
        "meta.if.condition.baml",
        String.raw`(?=\})`,
      ),
    ],
  },
  interpolationKeyword(
    "backtick-else",
    "meta.interpolation.control.else.baml",
    "else",
    "keyword.control.conditional.baml",
  ),
  interpolationKeyword(
    "backtick-endfor",
    "meta.interpolation.control.endfor.baml",
    "endfor",
    "keyword.control.loop.endfor.baml",
  ),
  interpolationKeyword(
    "backtick-endif",
    "meta.interpolation.control.endif.baml",
    "endif",
    "keyword.control.conditional.endif.baml",
  ),
  {
    key: "backtick-expression-interpolation",
    scope: "meta.interpolation.baml",
    begin: String.raw`(\$\{)`,
    beginCaptures: {
      "1": { scope: "punctuation.section.interpolation.begin.baml" },
    },
    end: String.raw`\}`,
    endCaptures: caps0("punctuation.section.interpolation.end.baml"),
    patterns: [
      comments,
      {
        key: "backtick-interpolation-let-statement",
        scope: "meta.statement.let.baml",
        begin: String.raw`(?=${BINDING_INTRO}\b)`,
        end: String.raw`(?=;|\})`,
        patterns: [
          comments,
          bindingPatternRule("backtick-interpolation-let-pattern", "meta.pattern.statement.baml", false),
          assignmentOperator,
          expression,
        ],
      },
      blockContents,
      expression,
      semicolon,
    ],
  },
];

// --- Classes ---------------------------------------------------------------

// `<name> <type>` inside a class body. The colon is optional: the parser accepts
// both `name string` and `name: string` (parse_field eats an optional Colon), so
// scope it only when present. Inline one-field classes (`class E { code int }`)
// need the `{` branch in addition to the normal start-of-line branch.
const field: Rule = {
  key: "field",
  scope: "meta.field.baml",
  begin: String.raw`(?:^\s*|(?<=\{)\s*)(?!@)(${IDENT})\s*(:)?`,
  beginCaptures: {
    "1": { scope: "variable.other.property.baml" },
    "2": { scope: "punctuation.separator.colon.baml" },
  },
  end: String.raw`(?=\})|$`,
  patterns: [comments, typeExpression],
};

const functionParameters: Rule = {
  key: "function-parameters",
  scope: "meta.parameters.baml",
  begin: String.raw`\(`,
  beginCaptures: caps0("punctuation.definition.parameters.begin.baml"),
  end: String.raw`\)`,
  endCaptures: caps0("punctuation.definition.parameters.end.baml"),
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
      patterns: [
        comments,
        {
          key: "parameter-type-parens",
          scope: "meta.group.type.baml",
          begin: String.raw`\(`,
          beginCaptures: caps0("punctuation.definition.type.begin.baml"),
          end: String.raw`\)`,
          endCaptures: caps0("punctuation.definition.type.end.baml"),
          patterns: [comments, typeExpression],
        },
        typeArgumentsRule("parameter-type-arguments", ""),
        {
          key: "parameter-default",
          scope: "meta.parameter.default.baml",
          begin: String.raw`=`,
          beginCaptures: caps0("keyword.operator.assignment.baml"),
          end: String.raw`(?=,|\))`,
          patterns: [comments, expression],
        },
        typeExpression,
      ],
    },
    comma,
  ],
};

const lambdaExpression: Rule = {
  key: "lambda-expression",
  scope: "meta.expression.lambda.baml",
  begin: String.raw`(?=\((?:[^()]|\([^()]*\))*\)\s*(?:->|=>))`,
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    functionParameters,
    {
      key: "lambda-arrow",
      scope: "keyword.operator.arrow.baml",
      match: String.raw`->|=>`,
    },
    {
      key: "lambda-return-type",
      scope: "meta.return-type.lambda.baml",
      begin: String.raw`(?<=->|=>)\s*(?!throws\b)(?=[^\s\{])`,
      end: String.raw`(?=\s*(?:throws\b|\{))`,
      patterns: [comments, typeExpression],
    },
    {
      key: "lambda-throws-clause",
      scope: "meta.throws.lambda.baml",
      begin: String.raw`\b(throws)\b`,
      beginCaptures: {
        "1": { scope: "keyword.operator.throws.baml" },
      },
      end: String.raw`(?=\{)`,
      patterns: [comments, typeExpression],
    },
    codeBlock,
  ],
};

expression.patterns = [
  literal,
  arrayExpression,
  lambdaExpression,
  parenthesizedExpression,
  ifExpression,
  matchExpression,
  spawnExpression,
  constructorExpression,
  mapExpression,
  codeBlock,
  awaitExpression,
  throwExpression,
  catchExpression,
  isPatternExpression,
  optionalCallExpression,
  optionalIndexExpression,
  optionalMethodCallExpression,
  optionalFieldAccessExpression,
  upcastExpression,
  postfixMethodCallExpression,
  fieldAccessExpression,
  expressionOperator,
  functionCallExpression,
  dottedExpression,
];

conditionExpression.patterns = expression.patterns
  .filter((rule) => rule !== constructorExpression)
  .map((rule) =>
    rule === isPatternExpression ? conditionIsPatternExpression : rule,
  );

// `-> Type` return clause; only the end differs between sites.
function returnTypeRule(key: string, end: string): Rule {
  return {
    key,
    scope: "meta.return-type.baml",
    begin: String.raw`(->)`,
    beginCaptures: { "1": { scope: "keyword.operator.arrow.baml" } },
    end,
    patterns: [comments, typeExpression],
  };
}

const functionReturnType: Rule = returnTypeRule(
  "function-return-type",
  String.raw`(?=\{)`,
);

const declarationTypeParameters: Rule = {
  key: "declaration-type-parameters",
  scope: "meta.type-parameters.baml",
  begin: String.raw`<`,
  beginCaptures: caps0("punctuation.definition.type-parameters.begin.baml"),
  end: String.raw`>(?=\s*(?:\(|\{|requires\b|${IDENT}))`,
  endCaptures: caps0("punctuation.definition.type-parameters.end.baml"),
  patterns: [
    comments,
    {
      key: "declaration-type-parameter",
      scope: "meta.type-parameter.baml",
      begin: String.raw`\b(${IDENT})\b`,
      beginCaptures: {
        "1": { scope: "entity.name.type.parameter.baml" },
      },
      end: String.raw`(?=,|>(?=\s*(?:\(|\{|requires\b|${IDENT})))`,
      patterns: [
        comments,
        {
          key: "declaration-type-parameter-extends",
          scope: "keyword.operator.extends.baml",
          match: String.raw`\bextends\b`,
        },
        {
          key: "declaration-type-parameter-intersection",
          scope: "keyword.operator.type.baml",
          match: String.raw`&`,
        },
        typeExpression,
      ],
    },
    comma,
  ],
};

// A `<keyword> ...` statement: STATEMENT_START + the keyword, ending at the
// statement terminator. `patterns`/`end` default to a bare keyword statement.
function keywordStatement(
  key: string,
  metaScope: BamlScope,
  keyword: string,
  kwScope: BamlScope,
  patterns: Rule[] = [comments],
  end: string = STATEMENT_END,
): Rule {
  return {
    key,
    scope: metaScope,
    begin: String.raw`${STATEMENT_START}(${keyword})\b`,
    beginCaptures: { "1": { scope: kwScope } },
    end,
    patterns,
  };
}

const returnStatement: Rule = keywordStatement(
  "return-statement",
  "meta.statement.return.baml",
  "return",
  "keyword.control.flow.return.baml",
  [comments, expression],
);

const breakStatement: Rule = keywordStatement(
  "break-statement",
  "meta.statement.break.baml",
  "break",
  "keyword.control.flow.break.baml",
);

const continueStatement: Rule = keywordStatement(
  "continue-statement",
  "meta.statement.continue.baml",
  "continue",
  "keyword.control.flow.continue.baml",
);

const deferStatement: Rule = keywordStatement(
  "defer-statement",
  "meta.statement.defer.baml",
  "defer",
  "keyword.control.flow.defer.baml",
  [comments, codeBlock],
  String.raw`(?<=\})`,
);

const letStatement: Rule = {
  key: "let-statement",
  scope: "meta.statement.let.baml",
  begin: String.raw`${STATEMENT_START}(?=${BINDING_INTRO}\b)`,
  end: STATEMENT_END,
  patterns: [
    comments,
    bindingPatternRule("let-statement-pattern", "meta.pattern.statement.baml", false),
    assignmentOperator,
    letElseClause,
    expression,
  ],
};

const watchStatement: Rule = keywordStatement(
  "watch-statement",
  "meta.statement.watch.baml",
  "watch",
  "keyword.control.watch.baml",
  [
    comments,
    bindingPatternRule("watch-statement-pattern", "meta.pattern.watch.baml", true),
    assignmentOperator,
    expression,
  ],
);

const configBlock: IncludeRule = {
  key: "config-block",
  patterns: [],
};

const configArray: IncludeRule = {
  key: "config-array",
  patterns: [],
};

configBlock.patterns = [
  braceBlock("config-block-body", "meta.config.block.baml", [
    comments,
    {
      key: "config-field",
      scope: "meta.field.config.baml",
      begin: String.raw`(?:^\s*|(?<=[\{,])\s*)(?:(${IDENT})\b|("(?:\\.|[^"\\])*"))\s*(:)?`,
      beginCaptures: {
        "1": { scope: "variable.other.property.baml" },
        "2": { patterns: [stringLiteral] },
        "3": { scope: "punctuation.separator.colon.baml" },
      },
      end: String.raw`(?=,|\r?\n|\})`,
      patterns: [comments, configBlock, configArray, expression],
    },
    comma,
  ]),
];

configArray.patterns = [
  {
    key: "config-array-body",
    scope: "meta.config.array.baml",
    begin: String.raw`\[`,
    beginCaptures: caps0("punctuation.definition.array.begin.baml"),
    end: String.raw`\]`,
    endCaptures: caps0("punctuation.definition.array.end.baml"),
    patterns: [
      comments,
      configBlock,
      configArray,
      expression,
      comma,
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

const templateStringItem: Rule = {
  key: "template-string",
  scope: "meta.template-string.baml",
  begin: String.raw`^\s*(template_string)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.template-string.baml" },
  },
  end: String.raw`(?:(?<=#)|(?<="))(?=\s*(?:\r?\n|$))|(?=${TOP_LEVEL_ITEM_START})`,
  patterns: [
    comments,
    {
      key: "template-string-name",
      scope: "entity.name.function.template-string.baml",
      match: String.raw`\b${IDENT}\b`,
    },
    functionParameters,
    templateStringBody,
  ],
};

const enumHeader: Rule = {
  key: "enum-header",
  scope: "meta.enum.header.baml",
  begin: String.raw`\G\s*`,
  end: HEADER_END,
  patterns: [
    comments,
    {
      key: "enum-name",
      scope: "entity.name.type.enum.baml",
      match: String.raw`\b${IDENT}\b`,
    },
  ],
};

const enumVariant: Rule = {
  key: "enum-variant",
  scope: "meta.enum.variant.baml",
  begin: String.raw`(?:^\s*|(?<=[\{,])\s*|\s+)(${IDENT})\b`,
  beginCaptures: {
    "1": { scope: "variable.other.enummember.baml" },
  },
  end: String.raw`(?=,|\s+@@|${ITEM_END_TAIL})`,
  patterns: [comments, bareAttribute, attribute],
};

const enumBody: Rule = braceBlock(
  "enum-body",
  "meta.enum.body.baml",
  [comments, blockAttribute, enumVariant, comma],
  String.raw`\}|(?=${TOP_LEVEL_ITEM_START})`,
);

const enumItem: Rule = {
  key: "enum",
  scope: "meta.enum.baml",
  begin: String.raw`^\s*(enum)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.enum.baml" },
  },
  end: ITEM_BODY_END,
  patterns: [comments, enumHeader, enumBody],
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

const testItem: Rule = {
  key: "test",
  scope: "meta.test.baml",
  begin: String.raw`^\s*(test)\b(?!(?:[^\S\r\n]+${IDENT}[^\S\r\n]*\{[^\S\r\n]*(?:functions|type_builder)\b))`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.test.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [comments, testHeader, codeBlock],
};

const testsetItem: Rule = {
  key: "testset",
  scope: "meta.testset.baml",
  begin: String.raw`^\s*(testset)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.testset.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    testHeader,
    braceBlock("testset-body", "meta.testset.body.baml", [comments, blockContents]),
  ],
};

blockContents.patterns = [
  comments,
  testsetItem,
  testItem,
  watchStatement,
  letStatement,
  returnStatement,
  breakStatement,
  continueStatement,
  deferStatement,
  elseClause,
  forStatement,
  whileStatement,
  expression,
  semicolon,
];

const functionBlock: Rule = braceBlock("function-block", "meta.block.function.baml", [
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
    patterns: [comments, templateStringBody, backtickStringLiteral, expression],
  },
  blockContents,
]);

const functionItem: Rule = {
  key: "function",
  scope: "meta.function.baml",
  begin: String.raw`^\s*(function)\s+(${IDENT})\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.function.baml" },
    "2": { scope: "entity.name.function.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    declarationTypeParameters,
    functionParameters,
    functionReturnType,
    functionBlock,
  ],
};

const associatedTypeItem: Rule = {
  key: "associated-type",
  scope: "meta.associated-type.baml",
  begin: String.raw`(?:^\s*|(?<=\{)\s*)(type)\s+(${IDENT})\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.associated-type.baml" },
    "2": { scope: "entity.name.type.associated.baml" },
  },
  end: MEMBER_SIGNATURE_END,
  patterns: [
    comments,
    {
      key: "associated-type-extends",
      scope: "keyword.operator.extends.baml",
      match: String.raw`\bextends\b`,
    },
    assignmentOperator,
    typeExpression,
    semicolon,
  ],
};

const interfaceMethodSignature: Rule = {
  key: "interface-method-signature",
  scope: "meta.function.signature.baml",
  begin: String.raw`^\s*(function)\s+(${IDENT})\b(?=[^{\r\n]*(?:\r?\n|;|\}))`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.function.baml" },
    "2": { scope: "entity.name.function.baml" },
  },
  end: MEMBER_SIGNATURE_END,
  patterns: [
    comments,
    declarationTypeParameters,
    functionParameters,
    returnTypeRule("interface-method-return-type", String.raw`(?=;|\r?\n|\})`),
    semicolon,
  ],
};

const interfaceRequiresClause: Rule = {
  key: "interface-requires-clause",
  scope: "meta.interface.requires.baml",
  begin: String.raw`\b(requires)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.requires.baml" },
  },
  end: String.raw`(?=\{)`,
  patterns: [
    comments,
    typeExpression,
    comma,
  ],
};

const interfaceFieldLink: Rule = {
  key: "interface-field-link",
  scope: "meta.interface-field-link.baml",
  begin: String.raw`(?:^\s*|(?<=\{)\s*)(${IDENT})\s+(as)\s+(${IDENT})\b`,
  beginCaptures: {
    "1": { scope: "variable.other.property.interface.baml" },
    "2": { scope: "keyword.operator.as.baml" },
    "3": { scope: "variable.other.property.baml" },
  },
  end: MEMBER_SIGNATURE_END,
  patterns: [comments, semicolon],
};

// `\G`-anchored type position in an implements header; scope + end vary.
function typeTargetClause(key: string, scope: BamlScope, end: string): Rule {
  return { key, scope, begin: String.raw`\G\s*`, end, patterns: [comments, typeExpression] };
}

const implementsBody: Rule = braceBlock("implements-body", "meta.implements.body.baml", [
  comments,
  functionItem,
  associatedTypeItem,
  interfaceFieldLink,
  field,
  semicolon,
  comma,
]);

const implementsBlock: Rule = {
  key: "implements-block",
  scope: "meta.implements.baml",
  begin: String.raw`^\s*(implements|implement)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.implements.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    typeTargetClause("implements-target", "meta.implements.target.baml", String.raw`(?=\{)`),
    implementsBody,
  ],
};

const implementsForItem: Rule = {
  key: "implements-for",
  scope: "meta.implements-for.baml",
  begin: String.raw`^\s*(implements|implement)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.implements.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    declarationTypeParameters,
    typeTargetClause(
      "implements-for-interface-target",
      "meta.implements.target.baml",
      String.raw`(?=\s+\bfor\b)`,
    ),
    {
      key: "implements-for-keyword",
      scope: "keyword.declaration.for.baml",
      match: String.raw`\bfor\b`,
    },
    typeTargetClause("implements-for-target", "meta.implements.for-target.baml", String.raw`(?=\{)`),
    implementsBody,
  ],
};

const interfaceItem: Rule = {
  key: "interface",
  scope: "meta.interface.baml",
  begin: String.raw`^\s*(interface)\s+(${IDENT})\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.interface.baml" },
    "2": { scope: "entity.name.type.interface.baml" },
  },
  end: String.raw`(?<=\})`,
  patterns: [
    comments,
    declarationTypeParameters,
    interfaceRequiresClause,
    braceBlock("interface-body", "meta.interface.body.baml", [
      comments,
      interfaceMethodSignature,
      functionItem,
      associatedTypeItem,
      field,
      semicolon,
      comma,
    ]),
  ],
};

const classHeader: Rule = {
  key: "class-header",
  scope: "meta.class.header.baml",
  begin: String.raw`\G\s*`,
  end: HEADER_END,
  patterns: [
    comments,
    declarationTypeParameters,
    {
      key: "class-name",
      scope: "entity.name.type.class.baml",
      match: String.raw`\b${IDENT}\b`,
    },
  ],
};

const classBody: Rule = braceBlock("class-body", "meta.class.body.baml", [
  comments,
  blockAttribute,
  bareAttribute,
  attribute,
  implementsBlock,
  functionItem,
  field,
  semicolon,
]);

const classItem: Rule = {
  key: "class",
  scope: "meta.class.baml",
  begin: String.raw`^\s*(class)\b`,
  beginCaptures: {
    "1": { scope: "keyword.declaration.class.baml" },
  },
  end: ITEM_BODY_END,
  patterns: [comments, classHeader, classBody],
};

// Hover/documentation fence fragments. The LSP renders single declaration
// fragments inside ```baml fences: a member line (a parameter's
// `asdf: Box<int>`, a field's `name: string`, a variant's `Active: Status`)
// or a bare owner/receiver type alone on a line (`T[]`, `map<K, V>`,
// `user.util.Widget<T>`). Neither is valid top-level BAML, but colorizing
// them keeps hover fences rendered like source — the same friendliness rule
// as the top-level `let` below. Both sit last in the root pattern list so
// every real declaration keyword wins first. (Deliberately not mirrored into
// the KDE syntax: Kate highlights files, not hover fences.)
const memberFragment: Rule = {
  key: "member-fragment",
  scope: "meta.member-fragment.baml",
  begin: String.raw`^\s*(${IDENT})\s*(\?)?\s*(:)`,
  beginCaptures: {
    "1": { scope: "variable.other.property.baml" },
    "2": { scope: "keyword.operator.optional.baml" },
    "3": { scope: "punctuation.separator.colon.baml" },
  },
  end: String.raw`$`,
  patterns: [comments, typeExpression],
};

const typeFragment: Rule = {
  key: "type-fragment",
  scope: "meta.type-fragment.baml",
  // Whole-line type material only, gated by three lookaheads: the line
  // starts with an identifier; it carries no member colon, initializer,
  // statement/block structure, call parens, or string delimiters; and it
  // contains a structural type character (`.<[?|`) or is exactly one
  // builtin primitive. The structural requirement keeps stray prose words
  // (`John Doe`, `Education`) plain while every receiver spelling the LSP
  // emits (`T[]`, `map<K, V>`, `user.util.Widget<T>`, `string`) qualifies.
  begin: String.raw`^\s*(?=${IDENT})(?=[^:=;{}()#"'\r\n]*$)(?=[^<.\[?|\r\n]*[<.\[?|]|(?:${oneOf(BUILTIN_TYPES)})\s*$)`,
  end: String.raw`$`,
  patterns: [comments, typeExpression],
};

export const baml: Grammar = {
  $schema: tm.schema,
  name: "baml",
  scopeName: "source.baml",
  fileTypes: ["baml"],
  patterns: [
    comments,
    clientItem,
    retryPolicyItem,
    generatorItem,
    templateStringItem,
    blockAttribute,
    enumItem,
    typeAliasItem,
    implementsForItem,
    interfaceItem,
    classItem,
    functionItem,
    testsetItem,
    testItem,
    // Top-level `let` / `const` bindings are not valid BAML, but highlighting
    // them (rather than leaving them as bare text) is friendlier while editing.
    // `semicolon` is their statement terminator, mirroring `blockContents`.
    letStatement,
    semicolon,
    // `associated-type` re-used at root covers the hover fence for an
    // associated-type declaration (`type Assoc extends Iface`) — the
    // `type X = …` alias rule above wins ties for real aliases.
    associatedTypeItem,
    memberFragment,
    typeFragment,
  ] satisfies Rule[],
};
