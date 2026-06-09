import type { Monaco } from '@monaco-editor/react';

/**
 * Registers a compact BAML language for Monaco: a Monarch tokenizer + a light
 * "paper" theme that matches the deck. Idempotent (safe to call on every mount).
 */
let registered = false;

export function registerBaml(monaco: Monaco) {
  if (registered) return;
  registered = true;

  monaco.languages.register({ id: 'baml' });

  monaco.languages.setLanguageConfiguration('baml', {
    comments: { lineComment: '//' },
    brackets: [
      ['{', '}'],
      ['[', ']'],
      ['(', ')'],
    ],
    autoClosingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
    ],
  });

  monaco.languages.setMonarchTokensProvider('baml', {
    defaultToken: '',
    keywords: [
      'function',
      'class',
      'enum',
      'type',
      'test',
      'testset',
      'client',
      'retry_policy',
      'generator',
      'let',
      'if',
      'else',
      'for',
      'while',
      'in',
      'break',
      'continue',
      'return',
      'throw',
      'throws',
      'match',
      'catch',
      'catch_all',
      'spawn',
      'with',
      'self',
      'instanceof',
      'dynamic',
      'watch',
      'import',
      'provider',
      'options',
      'prompt',
    ],
    typeKeywords: [
      'int',
      'float',
      'string',
      'bool',
      'image',
      'audio',
      'null',
      'true',
      'false',
      'map',
    ],
    tokenizer: {
      root: [
        [/\/\/.*$/, 'comment'],
        [/#"/, { token: 'string', next: '@blockstring' }],
        [/"/, { token: 'string', next: '@string' }],
        [
          /[a-z_]\w*/,
          {
            cases: {
              '@keywords': 'keyword',
              '@typeKeywords': 'type',
              '@default': 'identifier',
            },
          },
        ],
        [/[A-Z]\w*/, 'type.identifier'],
        [/@?\d+(\.\d+)?/, 'number'],
        [/[{}()[\]]/, '@brackets'],
        [/[=!<>|&+\-*/%.,;:?]/, 'operator'],
      ],
      string: [
        [/[^"]+/, 'string'],
        [/"/, { token: 'string', next: '@pop' }],
      ],
      // Raw block strings #"..."# — also light up Jinja {{ }} inside prompts.
      blockstring: [
        [/"#/, { token: 'string', next: '@pop' }],
        [/\{\{/, { token: 'variable', next: '@jinja' }],
        [/[^"{]+/, 'string'],
        [/["{]/, 'string'],
      ],
      jinja: [
        [/\}\}/, { token: 'variable', next: '@pop' }],
        [/[^}]+/, 'variable'],
      ],
    },
  });

  monaco.editor.defineTheme('baml-paper', {
    base: 'vs',
    inherit: true,
    rules: [
      // Keywords are blue, not GitHub red — red is reserved for diagnostics
      // (squiggles + inline error-lens), so nothing healthy reads as an error.
      { token: 'keyword', foreground: '0550AE' },
      { token: 'type', foreground: '953800' },
      { token: 'type.identifier', foreground: '953800' },
      { token: 'string', foreground: '0A3069' },
      { token: 'comment', foreground: '6E7781' },
      { token: 'number', foreground: '0550AE' },
      { token: 'variable', foreground: '6D28D9' },
      { token: 'operator', foreground: '24292E' },
      { token: 'identifier', foreground: '24292E' },
    ],
    colors: {
      'editor.background': '#FFFDF7',
      'editor.foreground': '#24292E',
      'editorLineNumber.foreground': '#B9B2A3',
      'editorLineNumber.activeForeground': '#6F6A63',
      'editor.lineHighlightBackground': '#F4EEE0',
      'editor.selectionBackground': '#E6D9FA',
      'editorCursor.foreground': '#6D28D9',
      'editorIndentGuide.background1': '#EFE8D8',
    },
  });
}
