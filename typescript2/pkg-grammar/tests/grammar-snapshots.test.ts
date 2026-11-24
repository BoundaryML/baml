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

  it('highlights tuple types, patterns, and slot access', () => {
    const leafScopes = (source: string) => {
      const { tokens } = highlighter.codeToTokens(source, {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      });
      return tokens
        .flat()
        .flatMap(explanationParts)
        .filter((part) => part.content.trim() !== '')
        .map((part) => [
          part.content.trim(),
          part.scopes?.at(-1)?.scopeName.replace(/\.baml$/, ''),
        ]);
    };
    const inFunction = (body: string) =>
      leafScopes(`function F() -> int {\n${body}\n}`).slice(7, -1);

    const PROPERTY = 'variable.other.property';
    const ACCESSOR = 'punctuation.accessor';
    expect(inFunction('t.0')).toEqual([
      ['t', 'variable.other.readwrite'],
      ['.', ACCESSOR],
      ['0', PROPERTY],
    ]);
    // The lexer reads `0.1` as one float; it is still two slots.
    expect(inFunction('t?.0.1')).toEqual([
      ['t', 'variable.other.readwrite'],
      ['?.', ACCESSOR],
      ['0', PROPERTY],
      ['.', ACCESSOR],
      ['1', PROPERTY],
    ]);
    expect(inFunction('xs[0].1 + 1.5')).toContainEqual(['1', PROPERTY]);
    expect(inFunction('xs[0].1 + 1.5')).toContainEqual([
      '1.5',
      'constant.numeric.float',
    ]);
    expect(inFunction('[1..2]')).not.toContainEqual(['2', PROPERTY]);

    for (const intro of ['let', 'const']) {
      expect(inFunction(`${intro} (${intro} a, (let b, _)) = p;`)).toEqual(
        expect.arrayContaining([
          [intro, 'keyword.declaration.binding'],
          ['a', 'variable.other.binding'],
          ['b', 'variable.other.binding'],
          ['_', 'variable.language.wildcard'],
        ]),
      );
    }
    expect(inFunction('if let (0, let s) = p { s }')).toEqual(
      expect.arrayContaining([
        ['let', 'keyword.declaration.binding'],
        ['0', 'constant.numeric.integer'],
        ['s', 'variable.other.binding'],
      ]),
    );

    // A nested tuple type keeps the optional suffix inside the parameter.
    expect(
      leafScopes('function F(t: (int, (string, bool))?) -> (int,) { t }'),
    ).toEqual(
      expect.arrayContaining([
        ['?', 'keyword.operator.optional'],
        [')', 'punctuation.definition.parameters.end'],
      ]),
    );

    // `(cond) =>` in a guard is the arm arrow, not a lambda (parser's
    // MATCH_GUARD restriction), so the rest of the file stays in sync.
    const guarded = leafScopes(
      'function F() -> int {\n  match (b) {\n    Box { v: let n } if (ok(b)) => n,\n    _ => 1,\n  }\n}\nfunction G() -> int { 1 }',
    );
    expect(guarded).toContainEqual(['ok', 'entity.name.function']);
    expect(guarded).toContainEqual(['=>', 'keyword.operator.arrow']);
    expect(guarded).toContainEqual(['G', 'entity.name.function']);
    expect(guarded).not.toContainEqual(['ok', 'variable.parameter']);

    const scrutinee = inFunction('match ((a, b): (int, string)) { _ => 1 }');
    expect(scrutinee).toContainEqual(['int', 'support.type.primitive']);
    expect(scrutinee).toContainEqual(['string', 'support.type.primitive']);

    expect(inFunction('match t?.1.0 { _ => 0 }')).toEqual(
      expect.arrayContaining([
        ['1', PROPERTY],
        ['0', PROPERTY],
      ]),
    );

    // Tuple types inside a call's type arguments.
    const typeArgs = inFunction('reflect.Type.of<(never, string)>()');
    expect(typeArgs).toContainEqual([
      '<',
      'punctuation.definition.type-arguments.begin',
    ]);
    expect(typeArgs).toContainEqual(['never', 'support.type.primitive']);
    expect(typeArgs).toContainEqual(['string', 'support.type.primitive']);
    expect(typeArgs).toContainEqual(['of', 'entity.name.function']);

    // A parenthesized function or suffixed type in a pattern is a type, not a
    // tuple destructure.
    const tupleScope = (source: string) => {
      const { tokens } = highlighter.codeToTokens(source, {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      });
      return tokens
        .flat()
        .flatMap(explanationParts)
        .some((part) =>
          part.scopes?.some((s) => s.scopeName.includes('destructure.tuple')),
        );
    };
    expect(
      tupleScope('function F() -> int {\n  let f: (int) -> int = g;\n}'),
    ).toBe(false);
    expect(
      tupleScope(
        'function F() -> int {\n  match (x) {\n    (int) -> int => 1,\n    (int | string)[] => 2,\n    (int | string)? => 3,\n  }\n}',
      ),
    ).toBe(false);
    expect(
      tupleScope('function F() -> int {\n  let (let a, let b) = p;\n}'),
    ).toBe(true);
    // `as` inside a literal is not a `(T as I).Assoc` projection.
    const literalAs = inFunction('match v { ("as", let item) => item }');
    expect(literalAs).toContainEqual(['item', 'variable.other.binding']);
    expect(inFunction('match v { (T as I).Assoc => 1 }')).toContainEqual([
      'Assoc',
      'entity.name.type.associated',
    ]);
  });

  it('reads `upcast` as the interface view only in `.upcast<T>()`', () => {
    const scopeStacks = (source: string) => {
      const { tokens } = highlighter.codeToTokens(source, {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      });
      return tokens
        .flat()
        .flatMap(explanationParts)
        .filter((part) => part.content.trim() !== '')
        .map((part) => [
          part.content.trim(),
          (part.scopes ?? []).map((scope) => scope.scopeName).join(' '),
        ]);
    };
    const UPCAST = 'meta.expression.upcast.baml';
    const KEYWORD = 'keyword.operator.expression.upcast.baml';

    // Where `upcast` is a member, the scopes are those of any other member
    // name. A `<` that does not open type arguments followed by `(` must not
    // open the upcast context: it would swallow the code after it as types.
    const plain = 'upcasx';
    for (const body of [
      'let small = x.upcast < limit;\n  let next = small && true;\n  next',
      'if (x.upcast < limit) { 1 } else { 2 }',
      'f(x.upcast < a, b > c)',
      'let total = x.upcast + x.upcast() + x.upcast(1) + x?.upcast;\n  total',
      'self.upcast < other.upcast',
    ]) {
      const source = (name: string) =>
        `function F(x: T, limit: int) -> int {\n  ${body.replaceAll('upcast', name)}\n}\nfunction G() -> int {\n  1\n}`;
      const expected = scopeStacks(source(plain));
      const found = scopeStacks(source('upcast')).map(([text, stack]) => [
        text === 'upcast' ? plain : text,
        stack,
      ]);
      expect(found, body).toEqual(expected);
      expect(JSON.stringify(found), body).not.toContain(UPCAST);
    }

    // The view: `upcast` is the keyword, the type arguments and the empty
    // parentheses belong to it, and what follows is scoped as after a call.
    for (const receiver of ['x', 'self', 'a.b', 'make()', 'xs[0]']) {
      const view = scopeStacks(
        `function F() -> string {\n  let name = ${receiver}.upcast<Boxed<int>>().name;\n  name\n}\nfunction G() -> int {\n  1\n}`,
      );
      const at = view.findIndex(([text]) => text === 'upcast');
      expect(view[at][1], receiver).toContain(KEYWORD);
      const inside = view.filter(([, stack]) => stack.includes(UPCAST));
      expect(inside.map(([text]) => text), receiver).toEqual([
        '.',
        'upcast',
        '<',
        'Boxed',
        '<',
        'int',
        '>',
        '>',
        '(',
        ')',
      ]);
      const after = view.slice(at).map(([text]) => text);
      expect(after.slice(9, 12), receiver).toEqual(['.', 'name', ';']);
      expect(
        view.find(([text]) => text === 'G'),
        receiver,
      ).toEqual(['G', expect.stringContaining('entity.name.function')]);
    }
  });

  it('separates comma-separated fields on one line', () => {
    const scopes = (source: string) => {
      const { tokens } = highlighter.codeToTokens(source, {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      });
      return tokens
        .flat()
        .flatMap(explanationParts)
        .filter((part) => part.content.trim() !== '')
        .map((part) => [
          part.content.trim(),
          part.scopes?.at(-1)?.scopeName.replace(/\.baml$/, ''),
        ]);
    };
    const PROPERTY = 'variable.other.property';

    const inline = scopes(
      'class X { first: map<map<string, int>, (int, bool)>, second: string @alias("a, b"), third int }',
    );
    for (const name of ['first', 'second', 'third']) {
      expect(inline).toContainEqual([name, PROPERTY]);
    }
    // Commas inside type arguments, tuple types, and attribute arguments do
    // not end the field.
    for (const name of ['map', 'int', 'bool', 'string']) {
      expect(inline).not.toContainEqual([name, PROPERTY]);
    }
    expect(inline).toContainEqual([',', 'punctuation.separator.comma']);

    const multiline = scopes(
      'class Y {\n  a map<map<string, int>, int>\n  b string\n}',
    );
    expect(multiline).toContainEqual(['a', PROPERTY]);
    expect(multiline).toContainEqual(['b', PROPERTY]);
  });

  it('uses one recursive brace context for map and block expressions', () => {
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
        'meta.expression.braced.baml',
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
      expect(openingScopes(body), body).toContain(
        'meta.expression.braced.baml',
      );
    }
    expect(openingScopes('map { key + 1: f() }')).toContain(
      'meta.expression.map.baml',
    );
  });

  it('reuses expression rules for multiline nested map keys and values', () => {
    const { tokens } = highlighter.codeToTokens(
      `let value = {
    f(
        [KeyKind.Foo],
        { "nested": g() }
    ): {
        key: h(),
    },
};`,
      {
        includeExplanation: 'scopeName',
        lang: 'baml',
        theme: THEME,
      },
    );
    const parts = tokens.flat().flatMap(explanationParts);
    const key = parts.find((part) => part.content === 'key');
    expect(key?.scopes?.at(-1)?.scopeName).toBe(
      'variable.other.readwrite.baml',
    );
    expect(
      key?.scopes?.filter(
        (scope) => scope.scopeName === 'meta.expression.braced.baml',
      ).length,
    ).toBe(2);
    for (const name of ['f', 'g', 'h']) {
      const call = parts.find((part) => part.content === name);
      expect(call?.scopes?.map((scope) => scope.scopeName)).toContain(
        'entity.name.function.baml',
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
