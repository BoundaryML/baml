// Contracts: the surface the agent is building. Every type, function, and
// field that changed since the session started, grouped by file in source
// order and shown as code: new records with all their fields, changed
// signatures with the old one struck through, fields added and removed. This
// is the view to read when the question is "what did the API become?".

import { memo, type ReactNode, useEffect, useMemo, useRef } from 'react';
import { CaptionBar } from '../components/caption';
import {
  ChangeTag,
  displayName,
  KindBadge,
  splitPath,
  withOffsets,
} from '../components/symbols';
import type { LedgerEntry } from '../engine/model';
import { useModel, usePlayer, useRuntime } from '../runtime';

export function ContractsView() {
  const model = useModel();
  const snap = usePlayer();
  const { focusRequest } = useRuntime();
  const scroller = useRef<HTMLDivElement>(null);

  const files = useMemo(() => {
    const byFile = new Map<string, LedgerEntry[]>();
    for (const entry of model.ledger(snap.played)) {
      // Document structure and config tables are not contracts.
      if (entry.change.kind === 'heading' || entry.change.kind === 'table')
        continue;
      const list = byFile.get(entry.path) ?? [];
      list.push(entry);
      byFile.set(entry.path, list);
    }
    return [...byFile.entries()]
      .map(([path, entries]) => ({
        entries: entries.sort((a, b) => lineOf(a) - lineOf(b)),
        last: Math.max(...entries.map((e) => e.lastEdit)),
        path,
        total: model.outlineAt(path, snap.played).length,
      }))
      .sort((a, b) => b.last - a.last);
  }, [model.version, snap.played]);

  // Keep the declaration the agent just touched in view.
  const latest = snap.played - 1;
  useEffect(() => {
    const el =
      scroller.current?.querySelector<HTMLElement>('[data-live="true"]');
    el?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }, [latest]);

  useEffect(() => {
    if (!focusRequest) return;
    const el = scroller.current?.querySelector<HTMLElement>(
      `[data-path="${CSS.escape(focusRequest.path)}"]`,
    );
    el?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, [focusRequest]);

  const activePath = snap.active?.edit.path;

  return (
    <>
      <div className="aa-contracts" ref={scroller}>
        {files.length === 0 && (
          <div className="aa-waiting">
            <h2>No declarations changed yet</h2>
            <p>
              Types, functions, and fields the agent adds or changes collect
              here, grouped by file.
            </p>
          </div>
        )}
        <div className="aa-contracts-grid">
          {files.map((f) => (
            <section
              aria-label={f.path}
              className="aa-contract-file"
              data-active={f.path === activePath}
              data-path={f.path}
              key={f.path}
            >
              <header className="aa-contract-file-head">
                <span className="path" title={f.path}>
                  {splitPath(f.path).dir}
                  <b>{splitPath(f.path).name}</b>
                </span>
                <span className="aa-text-3">
                  {f.entries.length} of {f.total} changed
                </span>
              </header>
              {f.entries.map((entry) => (
                <Declaration
                  entry={entry}
                  key={entry.change.key}
                  live={entry.lastEdit === latest}
                />
              ))}
            </section>
          ))}
        </div>
      </div>
      <CaptionBar />
    </>
  );
}

function lineOf(entry: LedgerEntry): number {
  return (entry.change.after ?? entry.change.before)?.start ?? 0;
}

const Declaration = memo(function Declaration({
  entry,
  live,
}: {
  entry: LedgerEntry;
  live: boolean;
}) {
  const c = entry.change;
  const record = c.after && c.after.members.length > 0;
  let body: ReactNode;
  if (c.change === 'removed') {
    body = (
      <span className="was">{highlight(c.before?.signature ?? c.name)}</span>
    );
  } else if (c.change === 'added') {
    body = (
      <>
        {highlight(c.after?.signature ?? c.name)}
        {record && (
          <>
            {' {'}
            {c.after?.members.map((m) => (
              <span className="member" data-kind="added" key={m.name}>
                {highlight(m.text)}
              </span>
            ))}
            {'}'}
          </>
        )}
      </>
    );
  } else {
    const changedMembers = new Set(c.members.map((m) => m.name));
    const unchanged = (c.after?.members ?? []).filter(
      (m) => !changedMembers.has(m.name),
    ).length;
    body = (
      <>
        {c.change === 'signature' && c.before && (
          <span className="was">{highlight(c.before.signature)}</span>
        )}
        {highlight(c.after?.signature ?? c.name)}
        {c.members.length > 0 && (
          <>
            {' {'}
            {c.members.map((m) => (
              <span className="member" data-kind={m.kind} key={m.name}>
                {m.kind === 'changed' && m.before ? (
                  <>
                    <span className="was">{m.before}</span>
                    {highlight(m.after ?? '')}
                  </>
                ) : (
                  highlight(m.after ?? m.before ?? m.name)
                )}
              </span>
            ))}
            {unchanged > 0 && (
              <span className="member aa-text-3">{`// ${unchanged} unchanged`}</span>
            )}
            {'}'}
          </>
        )}
      </>
    );
  }
  return (
    <div className="aa-decl" data-change={c.change} data-live={live}>
      <div className="aa-decl-head">
        <KindBadge kind={c.kind} />
        <span className="aa-sym-name">{displayName(c.name, c.container)}</span>
        <ChangeTag change={c.change} />
        {entry.touches > 1 && (
          <span className="aa-text-3 aa-decl-touches">
            {entry.touches} edits
          </span>
        )}
      </div>
      <pre className="aa-decl-code">{body}</pre>
    </div>
  );
});

const KEYWORDS = new Set([
  'class',
  'enum',
  'function',
  'fn',
  'pub',
  'struct',
  'interface',
  'type',
  'impl',
  'trait',
  'async',
  'export',
  'const',
  'client',
  'test',
  'def',
  'throws',
  'for',
  'where',
  'let',
  'mut',
  'self',
  'return',
  'default',
  'declare',
  'readonly',
  'static',
]);

/** Light token coloring for declaration text: keywords, type names, strings. */
function highlight(text: string): ReactNode {
  const parts = withOffsets(text.split(/("(?:[^"\\]|\\.)*"|[A-Za-z_][\w]*)/));
  return parts.map(({ part: p, offset: i }) => {
    if (!p) return null;
    if (p.startsWith('"'))
      return (
        <span className="j-str" key={i}>
          {p}
        </span>
      );
    if (KEYWORDS.has(p))
      return (
        <span className="tok-kw" key={i}>
          {p}
        </span>
      );
    if (/^[A-Z]/.test(p))
      return (
        <span className="tok-ty" key={i}>
          {p}
        </span>
      );
    return p;
  });
}
