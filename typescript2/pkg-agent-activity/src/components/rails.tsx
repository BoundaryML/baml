import { useMemo, useState } from 'react';
import type { LedgerEntry } from '../engine/model';
import { useModel, usePlayer, useRuntime } from '../runtime';
import { HoverHistory } from './caption';
import { formatMs, RunStatusIcon } from './run-card';
import {
  ChangeTag,
  Counts,
  displayName,
  KindBadge,
  splitPath,
} from './symbols';

/** Definitions listed under a file before the rest fold into "N more". */
const DEFS_PER_FILE = 5;

/**
 * Touched files, grouped by directory so each keeps its place in the project,
 * with the definitions the agent changed nested under each file: most
 * recently touched first, the first few shown.
 */
export function FileRail() {
  const model = useModel();
  const snap = usePlayer();
  const { focusFile, source } = useRuntime();
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [openDef, setOpenDef] = useState<string | null>(null);
  const files = model.touchedFiles(snap.played);
  const activePath =
    snap.active?.edit.path ?? model.edits[snap.played - 1]?.path;
  const now = source.now();
  const lastRun =
    snap.runsShown > 0 ? model.runs[snap.runsShown - 1] : undefined;
  const ranHere = new Set(lastRun?.touches ?? []);

  const groups = useMemo(() => {
    const byDir = new Map<string, string[]>();
    for (const f of files) {
      const { dir } = splitPath(f.path);
      const list = byDir.get(dir) ?? [];
      list.push(f.path);
      byDir.set(dir, list);
    }
    for (const list of byDir.values()) list.sort();
    return [...byDir.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [files.length, model.version]);

  const defsByFile = useMemo(() => {
    const out = new Map<string, LedgerEntry[]>();
    for (const entry of model.ledger(snap.played)) {
      if (entry.change.change === 'body' && entry.touches === 0) continue;
      const list = out.get(entry.path) ?? [];
      list.push(entry);
      out.set(entry.path, list);
    }
    for (const list of out.values()) list.sort(byRecency);
    return out;
  }, [model.version, snap.played]);

  const toggle = (path: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  return (
    <aside aria-label="Files" className="aa-rail aa-rail--left">
      <div className="aa-rail-head">
        Files <span>{files.length} touched</span>
      </div>
      {lastRun && (
        <button
          className="aa-last-run"
          onClick={() => lastRun.touches[0] && focusFile(lastRun.touches[0])}
          title={`${lastRun.command}\nThe marked files are the ones this run executed or reported on.`}
          type="button"
        >
          <RunStatusIcon status={lastRun.status} />
          <code>{lastRun.target?.name ?? lastRun.command}</code>
          <span>
            {lastRun.endedAt
              ? formatMs(lastRun.endedAt - lastRun.startedAt)
              : 'running'}
          </span>
        </button>
      )}
      <div className="aa-rail-scroll">
        {files.length === 0 && (
          <div className="aa-empty">
            Files appear here as the agent edits them.
          </div>
        )}
        {groups.map(([dir, paths]) => (
          <div key={dir}>
            <div className="aa-dir" title={dir}>
              {dir || './'}
            </div>
            {paths.map((path) => {
              const stats = model.sessionStats(path, snap.played);
              const last = model.lastEditOf(path, snap.played);
              const lastEdit =
                last !== undefined ? model.edits[last] : undefined;
              const age = lastEdit
                ? now - lastEdit.time
                : Number.POSITIVE_INFINITY;
              const kind = model.deletedAt(path, snap.played)
                ? 'deleted'
                : 'present';
              const defs = defsByFile.get(path) ?? [];
              const open = expanded.has(path);
              const shown = open ? defs : defs.slice(0, DEFS_PER_FILE);
              return (
                <div className="aa-file-group" key={path}>
                  <HoverHistory path={path}>
                    <button
                      className="aa-file"
                      data-active={path === activePath}
                      data-kind={kind}
                      data-ran={ranHere.has(path) ? lastRun?.status : undefined}
                      onClick={() => focusFile(path)}
                      title={path}
                      type="button"
                    >
                      <span
                        className="aa-heat"
                        style={heatStyle(age, path === activePath)}
                      />
                      <span className="aa-file-name">
                        {splitPath(path).name}
                      </span>
                      <Counts added={stats.added} removed={stats.removed} />
                    </button>
                  </HoverHistory>
                  {shown.map((entry) => {
                    const id = `${entry.path}\u0000${entry.change.key}`;
                    const c = entry.change;
                    return (
                      <button
                        aria-expanded={openDef === id}
                        className="aa-def"
                        data-change={c.change}
                        data-fresh={entry.lastEdit === snap.played - 1}
                        key={id}
                        onClick={() => {
                          setOpenDef(openDef === id ? null : id);
                          focusFile(entry.path);
                        }}
                        type="button"
                      >
                        <span className="aa-def-line">
                          <KindBadge kind={c.kind} />
                          <span className="aa-def-name">
                            {displayName(c.name, c.container)}
                          </span>
                          <ChangeTag change={c.change} />
                        </span>
                        {openDef === id && <SignatureDetail entry={entry} />}
                      </button>
                    );
                  })}
                  {defs.length > DEFS_PER_FILE && (
                    <button
                      className="aa-def-more"
                      onClick={() => toggle(path)}
                      type="button"
                    >
                      {open
                        ? 'Show fewer'
                        : `${defs.length - DEFS_PER_FILE} more`}
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}

function byRecency(a: LedgerEntry, b: LedgerEntry): number {
  return b.lastEdit - a.lastEdit || a.change.name.localeCompare(b.change.name);
}

function heatStyle(ageMs: number, active: boolean) {
  if (active) return { background: 'var(--aa-agent)' };
  // Fades over five minutes of session time.
  const f = Math.max(0, 1 - ageMs / 300_000);
  if (f <= 0) return undefined;
  return {
    background: `color-mix(in srgb, var(--aa-agent) ${Math.round(20 + f * 60)}%, var(--aa-line-strong))`,
  };
}

export function SignatureDetail({ entry }: { entry: LedgerEntry }) {
  const c = entry.change;
  return (
    <span className="aa-sig">
      {c.change === 'signature' && c.before && (
        <span className="was">{c.before.signature}</span>
      )}
      {c.after && c.change !== 'removed' && (
        <span className="now">{c.after.signature}</span>
      )}
      {c.change === 'removed' && c.before && (
        <span className="was">{c.before.signature}</span>
      )}
      {c.members.map((m) => (
        <span className="aa-member" data-kind={m.kind} key={m.name}>
          {m.kind === 'added' ? '+ ' : m.kind === 'removed' ? '− ' : '~ '}
          {m.after ?? m.before}
        </span>
      ))}
    </span>
  );
}

export function sortLedger(entries: LedgerEntry[]): LedgerEntry[] {
  return [...entries].sort(byRecency);
}
