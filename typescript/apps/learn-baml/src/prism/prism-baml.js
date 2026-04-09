/**
 * Prism language definition for BAML
 * Based on the BAML TextMate grammar from the VSCode extension
 */
(function (Prism) {
  Prism.languages.baml = {
    comment: [
      {
        // Documentation comments
        pattern: /\/\/\/.*/,
        greedy: true,
        alias: 'doc-comment',
      },
      {
        // Single line comments
        pattern: /\/\/.*/,
        greedy: true,
      },
    ],

    // Block strings with raw string syntax #"..."# or ##"..."##
    'block-string': {
      pattern: /#{1,5}"[\s\S]*?"#{1,5}/,
      greedy: true,
      alias: 'string',
      inside: {
        // Jinja template expressions inside block strings
        interpolation: {
          pattern: /\{\{[\s\S]*?\}\}/,
          inside: {
            'interpolation-punctuation': {
              pattern: /^\{\{|\}\}$/,
              alias: 'punctuation',
            },
            // Variables and filters inside interpolations
            rest: {
              variable: /\b[a-z_]\w*\b/,
              keyword: /\b(?:ctx|output_format|this|_)\b/,
              punctuation: /[|.()]/,
            },
          },
        },
        // Jinja control flow
        'jinja-block': {
          pattern: /\{%[\s\S]*?%\}/,
          inside: {
            'jinja-punctuation': {
              pattern: /^\{%|%\}$/,
              alias: 'punctuation',
            },
            keyword:
              /\b(?:if|else|elif|endif|for|endfor|in|macro|endmacro|set|block|endblock)\b/,
          },
        },
        // Chat role markers
        'chat-role': {
          pattern: /\{#(?:system|user|assistant)(?:\([^)]*\))?}/,
          alias: 'keyword',
        },
      },
    },

    // Regular double-quoted strings
    string: {
      pattern: /"(?:[^"\\]|\\.)*"/,
      greedy: true,
    },

    // Attributes like @description, @alias, @check, etc.
    attribute: {
      pattern: /@{1,2}\w+/,
      alias: 'annotation',
    },

    // Type keywords and declarations
    'class-name': [
      {
        // Type declarations: class Foo, enum Bar
        pattern: /(\b(?:class|enum|override)\s+)\w+/,
        lookbehind: true,
      },
      {
        // Type references in type positions (after :, ->, or in generic positions)
        pattern:
          /(?<=:\s*|->?\s*|\|\s*|<\s*|\[\s*|,\s*)(?:[A-Z]\w*|\b(?:bool|int|float|string|null|image|audio|pdf)\b)/,
      },
      {
        // Standalone type references (PascalCase)
        pattern: /\b[A-Z]\w*\b/,
      },
    ],

    // Function and template_string declarations
    'function-definition': {
      pattern: /(\b(?:function|template_string)\s+)\w+/,
      lookbehind: true,
      alias: 'function',
    },

    // Client, generator, test, retry_policy declarations
    'config-type': {
      pattern: /\b(?:client|generator|retry_policy|test|printer)\s*(?:<\w+>)?/,
      inside: {
        keyword: /\b(?:client|generator|retry_policy|test|printer)\b/,
        'generic-type': {
          pattern: /<\w+>/,
          inside: {
            punctuation: /[<>]/,
            'class-name': /\w+/,
          },
        },
      },
    },

    // Keywords
    keyword: [
      // Declaration keywords
      /\b(?:class|enum|function|template_string|type|client|generator|retry_policy|test|printer|override|let)\b/,
      // Control flow (for future use)
      /\b(?:if|else|for|while|in)\b/,
      // Special function body keywords
      /\b(?:prompt|input|output)\b/,
    ],

    // Built-in types
    builtin: /\b(?:bool|int|float|string|null|image|audio|pdf|map)\b/,

    // Boolean literals
    boolean: /\b(?:true|false)\b/,

    // Null literal
    'null-literal': {
      pattern: /\bnull\b/,
      alias: 'keyword',
    },

    // Numbers
    number: /\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/,

    // Property names in config blocks
    property: {
      pattern: /\b(?:provider|model|api_key|base_url|options|default_client_mode|output_type|version|max_retries|strategy)\b/,
      alias: 'atrule',
    },

    // Arrow for return types
    arrow: {
      pattern: /->/,
      alias: 'punctuation',
    },

    // Type operators
    'type-operator': {
      pattern: /\?|\[\]|\|/,
      alias: 'operator',
    },

    // Variable/parameter names (after type context is handled)
    variable: {
      pattern: /\b[a-z_]\w*(?=\s*:)/,
      alias: 'property',
    },

    // Operators
    operator: /[=<>!]=?|[+\-*/%]|&&|\|\|/,

    // Punctuation
    punctuation: /[{}[\]();:,.<>]/,
  };

  // Alias for common variations
  Prism.languages.BAML = Prism.languages.baml;
})(Prism);
