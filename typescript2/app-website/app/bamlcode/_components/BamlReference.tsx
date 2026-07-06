'use client';

import { useCallback, useState } from 'react';
import { describeExpr } from '../_lib/runtime';
import { stdlibDoc } from '../_lib/stdlib-docs';

/**
 * A slide-out cheat sheet of the `baml_language` primitives a solver needs.
 * Every snippet here is verified to compile and run in the browser BexVM.
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
    title: 'Basics',
    snippets: [
      { code: 'let x = 0;', note: 'mutable binding' },
      { code: 'let nums: int[] = [];', note: 'typed binding' },
      { code: 'function Add(a: int, b: int) -> int {\n  return a + b;\n}' },
    ],
  },
  {
    title: 'Control flow',
    snippets: [
      { code: 'if (x > 0) {\n  ...\n} else {\n  ...\n}' },
      { code: 'while (i < n) {\n  i += 1;\n}' },
      { code: 'for (x in nums) {\n  ...\n}', note: 'iterate values' },
      { code: 'break;   continue;   return v;' },
    ],
  },
  {
    title: 'Arrays',
    snippets: [
      { code: 'nums.length()' },
      { code: 'nums[i]', note: 'read (0-indexed)' },
      { code: 'nums[i] = v', note: 'write' },
      { code: 'nums.push(x)' },
      { code: 'nums.pop()', note: 'returns T? (optional)' },
      { code: 'nums.slice(a, b)   nums.reverse()' },
      { code: 'nums.sort()', note: 'ascending; int[] only, not int[][]' },
    ],
  },
  {
    title: 'Maps (use string keys)',
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
  },
  {
    title: 'Strings',
    snippets: [
      { code: 's.length()' },
      { code: 's.chars()', note: 'returns string[] of characters' },
      { code: 's.char_at(i)   s.substring(a, b)' },
      { code: 's.split(",")   s.to_lower_case()' },
      { code: 'c.is_alphanumeric()   c == "a"' },
    ],
  },
  {
    title: 'Numbers',
    snippets: [
      { code: 'a / b', note: 'integer division' },
      { code: 'a % b', note: 'remainder' },
      { code: 'x.abs()   x.pow(e)   x.min(y)   x.max(y)' },
      { code: 'x & 1   x >> 1   x << 2   x ^ y', note: 'bitwise ops' },
    ],
  },
  {
    title: 'Optionals & null',
    snippets: [
      { code: 'let node: ListNode? = null;' },
      {
        code: 'if (x == null) {\n  return 0;\n}\n// x is non-null below',
        note: 'guard-and-return narrows the type',
      },
    ],
  },
  {
    title: 'Classes',
    snippets: [
      { code: 'class ListNode {\n  val int\n  next ListNode?\n}' },
      { code: 'ListNode { val: 1, next: null }', note: 'construct with { }' },
      { code: 'node.val   node.next' },
    ],
  },
];

export function BamlReference() {
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);

  const describe = useCallback(async () => {
    const q = query.trim();
    if (!q || busy) return;
    // Curated stdlib docs answer builtin lookups instantly (the worker's hover
    // has no builtin docs); fall back to a live hover for user symbols.
    const local = stdlibDoc(q);
    if (local) {
      setResult(local);
      return;
    }
    setBusy(true);
    setResult(null);
    try {
      setResult(await describeExpr(q));
    } finally {
      setBusy(false);
    }
  }, [query, busy]);

  return (
    <aside className="bc-refcol">
      <div className="bc-ref-head">
        <span className="font-mono">BAML syntax</span>
      </div>
      <div className="bc-ref-body">
          <div className="bc-ref-describe">
            <div className="bc-ref-describe-row">
              <span className="bc-ref-describe-prompt font-mono">
                baml describe
              </span>
              <input
                className="bc-ref-describe-input font-mono"
                value={query}
                spellCheck={false}
                placeholder="e.g. baml.json.stringify"
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') describe();
                }}
              />
              <button
                type="button"
                className="bc-btn bc-btn-secondary font-mono"
                onClick={describe}
                disabled={busy}
              >
                {busy ? '…' : 'go'}
              </button>
            </div>
            {result ? (
              <pre className="bc-ref-describe-out font-mono">{result}</pre>
            ) : (
              <p className="bc-ref-describe-hint">
                Look up any function, method, or symbol.
              </p>
            )}
          </div>

          {SECTIONS.map((section) => (
            <section key={section.title} className="bc-ref-section">
              <h3 className="bc-ref-title font-mono">{section.title}</h3>
              {section.snippets.map((s) => (
                <div key={s.code} className="bc-ref-snippet">
                  <pre className="font-mono">{s.code}</pre>
                  {s.note ? <span className="bc-ref-note">{s.note}</span> : null}
                </div>
              ))}
            </section>
          ))}
      </div>
    </aside>
  );
}
