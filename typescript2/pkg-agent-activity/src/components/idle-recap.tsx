// Agents spend most of a session between edits: reading, building, running
// tests. While nothing is being written, this card says how long it has been
// and what the last burst of edits changed, so an idle screen still answers
// "what just happened?".

import type { SymbolChange } from '../engine/outline';
import { useModel, usePlayer, useRuntime, useTicker } from '../runtime';
import {
  ChangeTag,
  displayName,
  formatDuration,
  KindBadge,
  splitPath,
} from './symbols';

/** Show the recap once no edit has arrived for this long (session time). */
const SHOW_AFTER_MS = 2500;

export function IdleRecap() {
  const model = useModel();
  const snap = usePlayer();
  const { source } = useRuntime();
  useTicker(500);

  if (snap.active || snap.played === 0 || !snap.following) return null;
  const last = model.edits[snap.played - 1];
  const idle = source.now() - last.time;
  if (idle < SHOW_AFTER_MS) return null;

  const burst = model.bursts[last.burst];
  const edits = model.edits.slice(
    burst.start,
    Math.min(burst.end, snap.played),
  );
  const files = [...new Set(edits.map((e) => e.path))];
  const changes = new Map<string, { path: string; change: SymbolChange }>();
  for (const e of edits) {
    for (const c of e.symbols) {
      const id = `${e.path}\u0000${c.key}`;
      const prev = changes.get(id);
      // Keep the strongest description: a declaration added in this burst stays "new".
      if (!prev || prev.change.change === 'body')
        changes.set(id, { change: c, path: e.path });
    }
  }
  const listed = [...changes.values()]
    .sort((a, b) => rank(a.change) - rank(b.change))
    .slice(0, 6);
  const span = edits.length > 1 ? (edits.at(-1)?.time ?? 0) - edits[0].time : 0;

  return (
    <div aria-live="polite" className="aa-recap">
      <div className="aa-recap-title">
        No edits for {formatDuration(idle)}
        {source.fastForwarding() && <span>skipping ahead</span>}
      </div>
      <div className="aa-recap-sub">
        Last burst: {edits.length} {edits.length === 1 ? 'edit' : 'edits'} to{' '}
        {files.length} {files.length === 1 ? 'file' : 'files'}
        {span > 1000 ? ` over ${formatDuration(span)}` : ''}
      </div>
      <ul>
        {listed.length > 0
          ? listed.map(({ path, change }) => (
              <li key={`${path}${change.key}`} title={path}>
                <KindBadge kind={change.kind} />
                <code>{displayName(change.name, change.container)}</code>
                <ChangeTag change={change.change} />
              </li>
            ))
          : files.slice(0, 6).map((path) => (
              <li key={path} title={path}>
                <span />
                <code>{splitPath(path).name}</code>
                <span />
              </li>
            ))}
      </ul>
    </div>
  );
}

function rank(c: SymbolChange): number {
  return { added: 0, body: 4, members: 2, removed: 3, signature: 1 }[c.change];
}
