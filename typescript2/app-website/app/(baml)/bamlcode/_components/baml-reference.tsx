'use client';

import { useMemo, useState } from 'react';
import { type DocMethod, type DocSymbol, searchSymbols } from '@/lib/baml-docs';

/**
 * The `baml describe` side panel: instant, offline search over the real BAML
 * stdlib (a snapshot of `baml describe`, see `lib/baml-docs`). Empty box shows a
 * hand-picked cheat sheet of the primitives a solver reaches for most.
 */

interface Snippet {
  code: string;
  note?: string;
}
interface Section {
  title: string;
  snippets: Snippet[];
}

const SECTIONS: Section[] = [
  {
    snippets: [
      { code: 'let x = 0;', note: 'mutable binding' },
      { code: 'let nums: int[] = [];', note: 'typed binding' },
      { code: 'function Add(a: int, b: int) -> int {\n  return a + b;\n}' },
    ],
    title: 'Basics',
  },
  {
    snippets: [
      { code: 'if (x > 0) {\n  ...\n} else {\n  ...\n}' },
      { code: 'while (i < n) {\n  i += 1;\n}' },
      { code: 'for (x in nums) {\n  ...\n}', note: 'iterate values' },
      { code: 'break;   continue;   return v;' },
    ],
    title: 'Control flow',
  },
  {
    snippets: [
      { code: 'nums.length()' },
      { code: 'nums[i]', note: 'read (0-indexed)' },
      { code: 'nums[i] = v', note: 'write' },
      { code: 'nums.push(x)' },
      { code: 'nums.pop()', note: 'returns T? (optional)' },
      { code: 'nums.slice(a, b)   nums.reverse()' },
      {
        code: 'nums.sort_by_key(k)   nums.sort_by(cmp)',
        note: 'returns a sorted copy (no bare .sort())',
      },
    ],
    title: 'Arrays',
  },
  {
    snippets: [
      { code: 'let m: map<string, int> = {};' },
      { code: 'm.set("a", 1);' },
      { code: 'm.get("a")', note: 'returns int? (null if absent)' },
      { code: 'm.has("a")   m.keys()' },
      {
        code: 'm.set(n.to_string(), true);',
        note: 'int keys are unsupported: convert to string',
      },
    ],
    title: 'Maps (use string keys)',
  },
  {
    snippets: [
      { code: 's.length()', note: 'number of characters' },
      { code: 's.at(i)', note: 'returns string? (null if out of range)' },
      { code: 's.slice(a, b)' },
      { code: 's.split(",")   s.to_lower_case()' },
      { code: 'c.is_alphanumeric()   c == "a"' },
    ],
    title: 'Strings',
  },
  {
    snippets: [
      { code: 'a / b', note: 'integer division' },
      { code: 'a % b', note: 'remainder' },
      { code: 'x.abs()   x.pow(e)   x.min(y)   x.max(y)' },
      { code: 'x & 1   x >> 1   x << 2   x ^ y', note: 'bitwise ops' },
      { code: 'n.to_string()', note: 'value to string' },
      { code: 'int.parse(s)', note: 'string to int' },
    ],
    title: 'Numbers',
  },
  {
    snippets: [
      { code: 'let node: ListNode? = null;' },
      {
        code: 'if (x == null) {\n  return 0;\n}\n// x is non-null below',
        note: 'guard-and-return narrows the type',
      },
    ],
    title: 'Optionals & null',
  },
  {
    snippets: [
      { code: 'class ListNode {\n  val int\n  next ListNode?\n}' },
      { code: 'ListNode { val: 1, next: null }', note: 'construct with { }' },
      { code: 'node.val   node.next' },
    ],
    title: 'Classes',
  },
];

function leaf(name: string): string {
  const i = name.lastIndexOf('.');
  return i < 0 ? name : name.slice(i + 1);
}

function DocCard({
  symbol,
  matchedMethod,
  expandMethods,
}: {
  symbol: DocSymbol;
  matchedMethod?: DocMethod;
  expandMethods: boolean;
}) {
  const sig = matchedMethod ? matchedMethod.signature : symbol.shape;
  const doc = matchedMethod ? matchedMethod.docstring : symbol.docstring;
  const title = matchedMethod
    ? `${symbol.name}.${matchedMethod.name}`
    : symbol.name;
  const kindLabel = matchedMethod
    ? matchedMethod.kind === 'static'
      ? 'static fn'
      : 'method'
    : symbol.kind;
  const methods = (symbol.methods ?? []).filter((m) => !m.name.startsWith('_'));

  return (
    <div className="bc-doc">
      <div className="bc-doc-head">
        <span
          className={`bc-doc-kind bc-doc-kind-${kindLabel.replace(' ', '-')}`}
        >
          {kindLabel}
        </span>
        <code className="bc-doc-name font-mono">{title}</code>
      </div>
      {sig ? <pre className="bc-doc-sig font-mono">{sig}</pre> : null}
      {doc ? <p className="bc-doc-doc">{doc}</p> : null}
      {!matchedMethod && expandMethods && methods.length ? (
        <div className="bc-doc-methods">
          {methods.map((m) => (
            <div className="bc-doc-method" key={`${m.kind}.${m.name}`}>
              <code className="bc-doc-method-sig font-mono">{m.signature}</code>
              {m.docstring ? (
                <span className="bc-doc-method-doc">{m.docstring}</span>
              ) : null}
            </div>
          ))}
        </div>
      ) : !matchedMethod && methods.length ? (
        <div className="bc-doc-methods-count">
          {methods.length} method{methods.length === 1 ? '' : 's'} (search the
          type name to expand)
        </div>
      ) : null}
      <div className="bc-doc-loc font-mono">
        {symbol.file}:{symbol.line}
      </div>
    </div>
  );
}

export function BamlReference() {
  const [query, setQuery] = useState('');
  const q = query.trim();
  const results = useMemo(() => (q ? searchSymbols(q, 25) : []), [q]);
  // Expand a class's full method list only when the query has narrowed to it
  // (single result, or the leaf name matches exactly) so the list stays scannable.
  const expandKey = useMemo(() => q.toLowerCase(), [q]);

  return (
    <aside className="bc-refcol">
      <div className="bc-ref-head">
        <span className="font-mono">BAML docs</span>
      </div>
      <div className="bc-ref-body">
        <div className="bc-ref-describe">
          <div className="bc-ref-describe-row">
            <span className="bc-ref-describe-prompt font-mono">
              baml describe
            </span>
            <input
              className="bc-ref-describe-input font-mono"
              onChange={(e) => setQuery(e.target.value)}
              placeholder="string, map, json.stringify…"
              spellCheck={false}
              value={query}
            />
          </div>
          {q ? (
            results.length ? (
              <div className="bc-doc-results">
                {results.map((hit) => (
                  <DocCard
                    expandMethods={
                      results.length === 1 ||
                      leaf(hit.symbol.name).toLowerCase() === expandKey
                    }
                    key={hit.symbol.name + (hit.method?.name ?? '')}
                    matchedMethod={hit.method}
                    symbol={hit.symbol}
                  />
                ))}
              </div>
            ) : (
              <p className="bc-ref-describe-hint">
                No stdlib symbol matches “{q}”.
              </p>
            )
          ) : (
            <p className="bc-ref-describe-hint">
              Search the BAML stdlib (260 symbols). Try a type, function, or
              method name.
            </p>
          )}
        </div>

        {q
          ? null
          : SECTIONS.map((section) => (
              <section className="bc-ref-section" key={section.title}>
                <h3 className="bc-ref-title font-mono">{section.title}</h3>
                {section.snippets.map((s) => (
                  <div className="bc-ref-snippet" key={s.code}>
                    <pre className="font-mono">{s.code}</pre>
                    {s.note ? (
                      <span className="bc-ref-note">{s.note}</span>
                    ) : null}
                  </div>
                ))}
              </section>
            ))}
      </div>
    </aside>
  );
}
