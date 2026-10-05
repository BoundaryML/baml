import { mkdirSync, readdirSync, readFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { createHighlighter, type ThemedToken } from 'shiki';
import { describe, expect, it } from 'vitest';
import bamlGrammar from '../baml.tmLanguage.json';

const THEME = 'github-dark';

type ScopeExplanation = {
  content: string;
  scopes?: { scopeName: string }[];
};

const highlighter = await createHighlighter({
  langs: [bamlGrammar as never],
  themes: [THEME],
});

const fixturesDir = join(import.meta.dirname, 'fixtures');
const snapshotsDir = join(import.meta.dirname, 'snapshots');

function fixturePaths(dir = fixturesDir): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);

    if (entry.isDirectory()) {
      return fixturePaths(path);
    }

    return entry.isFile() && entry.name.endsWith('.baml') ? [path] : [];
  });
}

function fixtureName(path: string) {
  return relative(fixturesDir, path).split(sep).join('/');
}

function explanationParts(token: ThemedToken): ScopeExplanation[] {
  return (
    (token.explanation as ScopeExplanation[] | undefined) ?? [
      { content: token.content, scopes: [] },
    ]
  );
}

function formatScopeSnapshot(source: string) {
  const { tokens } = highlighter.codeToTokens(source, {
    includeExplanation: 'scopeName',
    lang: 'baml',
    theme: THEME,
  });

  const rows: string[] = [];

  tokens.forEach((line, lineIndex) => {
    let column = 0;

    for (const token of line) {
      for (const part of explanationParts(token)) {
        const start = column;
        const end = start + part.content.length;
        column = end;

        if (/^\s*$/.test(part.content)) {
          continue;
        }

        const range = `${lineIndex + 1}:${start + 1}-${end + 1}`;
        const text = JSON.stringify(part.content).padEnd(18);
        const scopes = (part.scopes ?? []).map((scope) => scope.scopeName);

        rows.push(`${range.padEnd(12)} ${text} ${scopes.join(' ')}`);
      }
    }
  });

  return `${rows.join('\n')}\n`;
}

describe('BAML TextMate grammar', () => {
  mkdirSync(snapshotsDir, { recursive: true });
  const fixtures = fixturePaths().sort();

  it('highlights map keys as expressions without changing constructor fields', () => {
    const leafScope = (source: string, content: string) => {
      const { tokens } = highlighter.codeToTokens(source, {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      });
      const part = tokens
        .flat()
        .flatMap(explanationParts)
        .find((part) => part.content === content);
      expect(part, `missing token ${content}`).toBeDefined();
      return part?.scopes?.at(-1)?.scopeName;
    };
    expect(leafScope('let values = { key: 1 };', 'key')).toBe(
      'variable.other.readwrite.baml',
    );
    expect(leafScope('let values = { KeyKind.Foo: 1 };', 'KeyKind')).toBe(
      leafScope('let value = KeyKind.Foo;', 'KeyKind'),
    );
    expect(leafScope('let value = Point { key: 1 };', 'key')).toBe(
      'variable.other.property.baml',
    );
  });

  it('recognizes computed first keys without mistaking blocks for maps', () => {
    const openingScopes = (body: string) => {
      const { tokens } = highlighter.codeToTokens(`let value = ${body};`, {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      });
      return tokens
        .flat()
        .flatMap(explanationParts)
        .find((part) => part.content === '{')
        ?.scopes?.map((scope) => scope.scopeName);
    };
    for (const key of [
      'key + 1',
      'f()',
      '-key',
      'keys[0]',
      '(key + 1)',
      'f(g(h(key)))',
      'Point { x: 1, y: 2 }',
      'if (flag) { 1 } else { 2 }',
      'f("a:b}")',
      'f(#"a:b}"#)',
      'f(`a:b}`)',
      'f(``a:b}``)',
      'f(/* : } */ key)',
      'f<int, string>()',
    ]) {
      expect(openingScopes(`{ ${key}: 1 }`), key).toContain(
        'meta.expression.map.baml',
      );
    }
    for (const body of [
      '{ key + 1 }',
      '{ f("a:b}") }',
      '{ f(#"a:b}"#) }',
      '{ f(`a:b}`) }',
      '{ f(``a:b}``) }',
      '{ ``a:b}`` }',
      '{ f(/* : } */ key) }',
      '{ key // not a map: value',
      '{ let x: int = 1; x }',
      '{ /* comment */ let x: int = 1; x }',
      '{ (x: int) -> int { x } }',
      '{ Point { x: 1, y: 2 } }',
    ]) {
      expect(openingScopes(body), body).not.toContain(
        'meta.expression.map.baml',
      );
    }
  });

  for (const fixture of fixtures) {
    const name = fixtureName(fixture);

    it(`tokenizes ${name}`, async () => {
      const source = readFileSync(fixture, 'utf8');

      await expect(formatScopeSnapshot(source)).toMatchFileSnapshot(
        join(snapshotsDir, `${name}.scope.txt`),
      );
    });
  }
});
