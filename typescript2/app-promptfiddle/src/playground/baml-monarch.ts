/**
 * Monaco Monarch grammar for the BAML language.
 *
 * Provides syntax highlighting for BAML files in the Monaco editor.
 * This grammar covers keywords, strings, comments, type annotations,
 * template interpolations, and attributes.
 */

import type * as monaco from 'monaco-editor';

export const BAML_LANGUAGE_ID = 'baml';

export const bamlLanguageConfiguration: monaco.languages.LanguageConfiguration = {
  comments: {
    lineComment: '//',
    blockComment: ['/*', '*/'],
  },
  brackets: [
    ['{', '}'],
    ['[', ']'],
    ['(', ')'],
  ],
  autoClosingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: '"', close: '"', notIn: ['string'] },
    { open: "'", close: "'", notIn: ['string'] },
  ],
  surroundingPairs: [
    { open: '{', close: '}' },
    { open: '[', close: ']' },
    { open: '(', close: ')' },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
  folding: {
    markers: {
      start: /^\s*\{/,
      end: /^\s*\}/,
    },
  },
};

export const bamlMonarchLanguage: monaco.languages.IMonarchLanguage = {
  defaultToken: '',
  tokenPostfix: '.baml',

  // BAML keywords
  keywords: [
    'function', 'class', 'enum', 'client', 'generator', 'test',
    'retry_policy', 'template_string', 'type_builder',
    'if', 'else', 'for', 'while', 'let', 'in',
    'break', 'continue', 'return', 'match', 'assert',
    'watch', 'instanceof', 'env', 'dynamic',
    'true', 'false', 'null',
  ],

  // Primitive types
  typeKeywords: [
    'int', 'float', 'string', 'bool', 'null', 'image', 'audio',
  ],

  operators: [
    '=', '==', '!=', '<', '>', '<=', '>=',
    '+', '-', '*', '/',
    '&&', '||', '!',
    '->', '=>', '|', '?', '.',
    '@@', '@',
  ],

  symbols: /[=><!~?:&|+\-*/^%@#]+/,

  escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

  tokenizer: {
    root: [
      // Comments
      [/\/\/.*$/, 'comment'],
      [/\/\*/, 'comment', '@blockComment'],

      // Raw strings (#"..."#)
      [/#+/, { token: 'string.delim', next: '@rawStringPrefix' }],

      // Attributes (@@dynamic, @alias, etc.)
      [/@@[a-zA-Z_]\w*/, 'annotation'],
      [/@[a-zA-Z_]\w*/, 'annotation'],

      // Template interpolations {{ }}
      [/\{\{/, { token: 'delimiter.bracket', next: '@interpolation' }],

      // Identifiers & keywords
      [/[a-zA-Z_]\w*/, {
        cases: {
          '@keywords': 'keyword',
          '@typeKeywords': 'type',
          '@default': 'identifier',
        },
      }],

      // $-prefixed identifiers (builtins like $watch)
      [/\$[a-zA-Z_]\w*/, 'variable.predefined'],

      // Numbers
      [/\d*\.\d+([eE][-+]?\d+)?/, 'number.float'],
      [/\d+/, 'number'],

      // Strings
      [/"/, 'string', '@string'],

      // Whitespace
      [/[ \t\r\n]+/, 'white'],

      // Delimiters and operators
      [/[{}()[\]]/, '@brackets'],
      [/->/, 'operator'],
      [/=>/, 'operator'],
      [/@symbols/, {
        cases: {
          '@operators': 'operator',
          '@default': '',
        },
      }],

      // Comma, semicolons
      [/[;,]/, 'delimiter'],
    ],

    blockComment: [
      [/[^/*]+/, 'comment'],
      [/\*\//, 'comment', '@pop'],
      [/[/*]/, 'comment'],
    ],

    string: [
      [/[^\\"]+/, 'string'],
      [/@escapes/, 'string.escape'],
      [/\\./, 'string.escape.invalid'],
      [/"/, 'string', '@pop'],
    ],

    // After seeing '#', check for '"' to start a raw string
    rawStringPrefix: [
      [/"/, { token: 'string.delim', next: '@rawStringBody' }],
      // If not followed by ", pop back and treat the # as punctuation
      [/./, { token: 'delimiter', next: '@pop', goBack: 1 }],
    ],

    rawStringBody: [
      // Template interpolation inside raw strings
      [/\{\{/, { token: 'delimiter.bracket', next: '@interpolation' }],
      // End of raw string: "# (simplified — doesn't count hashes)
      [/"#+/, { token: 'string.delim', next: '@popall' }],
      [/[^"{}]+/, 'string'],
      [/./, 'string'],
    ],

    interpolation: [
      [/\}\}/, { token: 'delimiter.bracket', next: '@pop' }],
      [/[a-zA-Z_]\w*/, {
        cases: {
          '@keywords': 'keyword',
          '@default': 'identifier',
        },
      }],
      [/\./, 'delimiter'],
      [/[ \t\r\n]+/, 'white'],
      [/./, ''],
    ],
  },
};
