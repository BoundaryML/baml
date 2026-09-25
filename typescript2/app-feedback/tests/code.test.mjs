import { expect, test } from 'bun:test';
import { bamlTokens } from '../src/lib/code.ts';
import { investigationPrompt } from '../src/lib/investigation.ts';

test('BAML coloring preserves code, including hostile markup', () => {
  const source =
    'class Item { value string }\n// comment\nlet n = 42;\nlet s = "<script>alert(1)</script>";';
  const tokens = bamlTokens(source);
  expect(tokens.map((token) => token.text).join('')).toBe(source);
  for (const kind of ['keyword', 'type', 'comment', 'number', 'string'])
    expect(tokens.some((token) => token.kind === kind)).toBe(true);
  const huge = 'x'.repeat(100001);
  expect(bamlTokens(huge)).toEqual([{ text: huge }]);
});

const issue = {
  description: 'Investigate it',
  id: 'ISSUE-1',
  repros: [
    {
      command: 'baml check',
      expectation: { check: 'should_compile' },
      files: { 'baml_src/main.baml': 'function Main() -> int { 1 }' },
      setup: null,
    },
  ],
  title: 'Compiler report',
  version: '0.18.0',
};
test('investigation prompt pins the reported version and includes complete repro evidence', () => {
  const prompt = investigationPrompt(issue);
  for (const expected of [
    'baml toolchain install 0.18.0',
    'export BAML_VERSION=0.18.0',
    'baml --version',
    'baml_src/main.baml',
    'function Main() -> int { 1 }',
    'baml check',
    'should_compile',
  ])
    expect(prompt).toContain(expected);
  expect(prompt).not.toContain('baml toolchain use canary');
});
test('invalid versions never become shell commands and embedded fences cannot escape source blocks', () => {
  const prompt = investigationPrompt({
    ...issue,
    repros: [{ ...issue.repros[0], files: { 'a.baml': '```\n# untrusted' } }],
    version: '0.18.0; malicious',
  });
  expect(prompt).not.toContain('export BAML_VERSION=');
  expect(prompt).toContain('unknown or invalid');
  expect(prompt).toContain('````baml\n```\n# untrusted\n````');
});

test('Python coloring preserves multiline strings, comments and hostile source exactly', async () => {
  const { codeTokens } = await import('../src/lib/code.ts');
  const source = `import unittest\ntext = '''<script>\n# inside string\n'''\nassert text != None # comment\n`;
  const tokens = codeTokens(source, 'test_repro.py');
  expect(tokens.map((t) => t.text).join('')).toBe(source);
  expect(tokens.find((t) => t.text.startsWith("'''"))?.kind).toBe('string');
  expect(tokens.find((t) => t.text === '# comment')?.kind).toBe('comment');
  expect(codeTokens(source, 'text')).toEqual([{ text: source }]);
});
