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

  // Dark presets (see code-theme.tsx). Keywords stay blue-ish, red is still
  // reserved for diagnostics. Classic VS Code "Dark+" palette.
  monaco.editor.defineTheme('baml-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '569CD6' },
      { token: 'type', foreground: '4EC9B0' },
      { token: 'type.identifier', foreground: '4EC9B0' },
      { token: 'string', foreground: 'CE9178' },
      { token: 'comment', foreground: '6A9955' },
      { token: 'number', foreground: 'B5CEA8' },
      { token: 'variable', foreground: 'C586C0' },
      { token: 'operator', foreground: 'D4D4D4' },
      { token: 'identifier', foreground: 'D4D4D4' },
    ],
    colors: {
      'editor.background': '#1E1E1E',
      'editor.foreground': '#D4D4D4',
      'editorLineNumber.foreground': '#5A5A5A',
      'editorLineNumber.activeForeground': '#C6C6C6',
      'editor.lineHighlightBackground': '#2A2A2A',
      'editor.selectionBackground': '#3A3D5C',
      'editorCursor.foreground': '#A78BFA',
      'editorIndentGuide.background1': '#2A2A2A',
    },
  });

  // Cooler, deeper dark (Tokyo Night-ish) for comparison.
  monaco.editor.defineTheme('baml-midnight', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'keyword', foreground: '7AA2F7' },
      { token: 'type', foreground: '2AC3DE' },
      { token: 'type.identifier', foreground: '2AC3DE' },
      { token: 'string', foreground: '9ECE6A' },
      { token: 'comment', foreground: '565F89' },
      { token: 'number', foreground: 'FF9E64' },
      { token: 'variable', foreground: 'BB9AF7' },
      { token: 'operator', foreground: '89DDFF' },
      { token: 'identifier', foreground: 'C0CAF5' },
    ],
    colors: {
      'editor.background': '#1A1B26',
      'editor.foreground': '#C0CAF5',
      'editorLineNumber.foreground': '#3B4261',
      'editorLineNumber.activeForeground': '#737AA2',
      'editor.lineHighlightBackground': '#1F2233',
      'editor.selectionBackground': '#33467C',
      'editorCursor.foreground': '#C0CAF5',
      'editorIndentGuide.background1': '#1F2233',
    },
  });
}
