import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { compile } from 'monaco-editor/esm/vs/editor/standalone/common/monarch/monarchCompile.js';
import { MonarchTokenizer } from 'monaco-editor/esm/vs/editor/standalone/common/monarch/monarchLexer.js';
import * as introSnippets from '../app/baml-intro/_components/snippets.ts';
import { registerBaml } from '../app/learn2/_lib/baml-monarch.ts';
import * as learn5Snippets from '../app/learn5/_components/snippets.ts';

const appRoot = fileURLToPath(new URL('..', import.meta.url));
const typescriptRoot = join(appRoot, '..');

const BAML_SNIPPET_NAMES = /^(BAML_|BENCH_BAML$|NAV_CODEBASE$|NS_(BAD|GOOD)$)/;

function collectBamlFiles(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);

    if (entry.isDirectory()) {
      collectBamlFiles(path, out);
    } else if (entry.isFile() && entry.name.endsWith('.baml')) {
      out.push(path);
    }
  }

  return out;
}

function collectExamples() {
  const examples = [];

  for (const [moduleName, snippets] of [
    ['baml-intro snippets', introSnippets],
    ['learn5 snippets', learn5Snippets],
  ]) {
    for (const [name, source] of Object.entries(snippets)) {
      if (BAML_SNIPPET_NAMES.test(name) && typeof source === 'string') {
        examples.push({ name: `${moduleName}:${name}`, source });
      }
    }
  }

  for (const path of collectBamlFiles(appRoot).sort()) {
    examples.push({
      name: relative(appRoot, path),
      source: readFileSync(path, 'utf8'),
    });
  }

  for (const dir of [
    join(typescriptRoot, 'pkg-grammar', 'preview'),
    join(typescriptRoot, 'pkg-grammar', 'tests', 'fixtures'),
  ]) {
    if (!existsSync(dir)) continue;

    for (const path of collectBamlFiles(dir).sort()) {
      examples.push({
        name: relative(typescriptRoot, path),
        source: readFileSync(path, 'utf8'),
      });
    }
  }

  return examples;
}

function createTokenizer() {
  let language;

  registerBaml({
    editor: {
      defineTheme() {},
    },
    languages: {
      register() {},
      setLanguageConfiguration() {},
      setMonarchTokensProvider(id, provider) {
        assert.equal(id, 'baml');
        language = provider;
      },
    },
  });

  assert.ok(language, 'registerBaml must register a Monarch token provider');
  assert.equal(language.start, 'root');

  const lexer = compile('baml', language);
  assert.equal(lexer.start, 'root');

  return new MonarchTokenizer(
    {
      isRegisteredLanguageId: () => false,
      languageIdCodec: {
        encodeLanguageId: () => 1,
      },
      requestBasicLanguageFeatures() {},
    },
    {
      getColorTheme: () => ({
        tokenTheme: {
          match: () => 0,
        },
      }),
    },
    'baml',
    lexer,
    {
      getValue: () => 100_000,
      onDidChangeConfiguration: () => ({ dispose() {} }),
    },
  );
}

function tokenizeExample(tokenizer, example) {
  let state = tokenizer.getInitialState();
  const lines = example.source.split(/\r?\n/);

  for (const [index, line] of lines.entries()) {
    try {
      state = tokenizer.tokenizeEncoded(
        line,
        index < lines.length - 1,
        state,
      ).endState;
    } catch (error) {
      throw new Error(
        `${example.name}:${index + 1}: ${error.message}\n${line}`,
        { cause: error },
      );
    }
  }
}

function classicTokensFor(tokenizer, source) {
  let state = tokenizer.getInitialState();
  const tokens = [];

  for (const [lineIndex, line] of source.split(/\r?\n/).entries()) {
    const result = tokenizer.tokenize(line, true, state);
    state = result.endState;

    for (const [index, token] of result.tokens.entries()) {
      const endOffset = result.tokens[index + 1]?.offset ?? line.length;
      tokens.push({
        line: lineIndex + 1,
        text: line.slice(token.offset, endOffset),
        type: token.type,
      });
    }
  }

  return tokens;
}

function assertToken(tokens, expected) {
  assert.ok(
    tokens.some(
      (token) =>
        token.text === expected.text && token.type.startsWith(expected.type),
    ),
    `expected ${JSON.stringify(expected.text)} to be tokenized as ${expected.type}`,
  );
}

function assertTokenOnLine(tokens, expected) {
  assert.ok(
    tokens.some(
      (token) =>
        token.line === expected.line &&
        token.text === expected.text &&
        token.type.startsWith(expected.type),
    ),
    `expected ${JSON.stringify(expected.text)} on line ${expected.line} to be tokenized as ${expected.type}`,
  );
}

test('BAML Monarch tokenizer does not throw on examples and fixtures', (t) => {
  const tokenizer = createTokenizer();
  const examples = collectExamples();

  assert.ok(examples.length > 0, 'expected at least one BAML example');
  t.diagnostic(`tokenizing ${examples.length} BAML examples`);

  for (const example of examples) {
    tokenizeExample(tokenizer, example);
  }

  const tokens = classicTokensFor(tokenizer, introSnippets.BAML_UNKNOWN);
  assertToken(tokens, { text: 'load', type: 'entity.name.function' });
  assertToken(tokens, { text: 'name', type: 'variable.other.property' });
  assertToken(tokens, { text: 'email', type: 'variable.other.property' });
  assertToken(tokens, {
    text: 'to_lower_case',
    type: 'entity.name.function.method',
  });

  const portedGrammarTokens = classicTokensFor(
    tokenizer,
    [
      'class Inline { code int }',
      'template_string Welcome(name: string) #"Hello {{ name }} {# plain string #}"#',
      'function bytes() -> string { b"\\x41" }',
      'function cast(x: unknown) -> string { x.as<Foo>().name }',
      'function greet(name: string) -> string { `Hello $' +
        '{name.to_lower_case()}` }',
    ].join('\n'),
  );

  assertToken(portedGrammarTokens, {
    text: 'code',
    type: 'variable.other.property',
  });
  assertToken(portedGrammarTokens, {
    text: 'b',
    type: 'storage.type.string',
  });
  assertToken(portedGrammarTokens, {
    text: String.raw`\x41`,
    type: 'string.escape',
  });
  assertToken(portedGrammarTokens, {
    text: '${',
    type: 'variable',
  });
  assertToken(portedGrammarTokens, {
    text: 'to_lower_case',
    type: 'entity.name.function.method',
  });
  assertToken(portedGrammarTokens, {
    text: 'as',
    type: 'keyword.operator.as',
  });

  const matchTokens = classicTokensFor(
    tokenizer,
    [
      'function render(msg: Question | Refund | string) -> string {',
      '  match (msg) {',
      '    Refund => `refund $' + '{msg.id}`,',
      '    Question { text } => `answer: $' + '{text}`,',
      '    string => `text: $' + '{msg}`,',
      '    let answer: Question => answer.text,',
      '    Status.Done => "done",',
      '  }',
      '}',
    ].join('\n'),
  );

  assertTokenOnLine(matchTokens, {
    line: 3,
    text: 'Refund',
    type: 'type.identifier',
  });
  assertTokenOnLine(matchTokens, {
    line: 4,
    text: 'Question',
    type: 'type.identifier',
  });
  assertTokenOnLine(matchTokens, {
    line: 4,
    text: 'text',
    type: 'identifier',
  });
  assertTokenOnLine(matchTokens, {
    line: 5,
    text: 'string',
    type: 'type.identifier',
  });
  assertTokenOnLine(matchTokens, {
    line: 6,
    text: 'answer',
    type: 'variable.other.binding',
  });
  assertTokenOnLine(matchTokens, {
    line: 6,
    text: 'Question',
    type: 'type.identifier',
  });
  assertTokenOnLine(matchTokens, {
    line: 7,
    text: 'Done',
    type: 'type.identifier',
  });
});
