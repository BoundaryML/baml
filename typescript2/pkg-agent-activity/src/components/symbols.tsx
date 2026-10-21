import type { SymbolChangeKind, SymbolKind } from '../engine/outline';

const KIND_LETTER: Record<SymbolKind, string> = {
  class: 'C',
  client: 'Cl',
  const: 'K',
  enum: 'E',
  function: 'F',
  heading: '#',
  impl: 'Im',
  interface: 'I',
  method: 'M',
  module: 'Md',
  struct: 'S',
  table: '[]',
  template: 'Tp',
  test: 'Ts',
  trait: 'T',
  type: 'T',
};

export function kindFamily(kind: SymbolKind): 'type' | 'fn' | 'other' {
  switch (kind) {
    case 'class':
    case 'struct':
    case 'enum':
    case 'interface':
    case 'trait':
    case 'type':
      return 'type';
    case 'function':
    case 'method':
    case 'template':
      return 'fn';
    default:
      return 'other';
  }
}

export function KindBadge({ kind }: { kind: SymbolKind }) {
  return (
    <span className="aa-kind" data-family={kindFamily(kind)} title={kind}>
      {KIND_LETTER[kind]}
    </span>
  );
}

const CHANGE_LABEL: Record<SymbolChangeKind, string> = {
  added: 'new',
  body: 'body',
  members: 'fields',
  removed: 'removed',
  signature: 'signature',
};

export function ChangeTag({ change }: { change: SymbolChangeKind }) {
  return (
    <span className="aa-change-tag" data-change={change}>
      {CHANGE_LABEL[change]}
    </span>
  );
}

export function displayName(name: string, container?: string): string {
  return container ? `${container}.${name}` : name;
}

/** `a/b/c/file.rs` → `{ dir: 'a/b/c/', name: 'file.rs' }` */
export function splitPath(path: string): { dir: string; name: string } {
  const slash = path.lastIndexOf('/');
  return slash < 0
    ? { dir: '', name: path }
    : { dir: path.slice(0, slash + 1), name: path.slice(slash + 1) };
}

export function Counts({ added, removed }: { added: number; removed: number }) {
  return (
    <span className="aa-counts">
      {added > 0 && <span className="add">+{added}</span>}
      {removed > 0 && <span className="del">−{removed}</span>}
    </span>
  );
}

export function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, '0')}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, '0')}m`;
}

export function formatClock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/**
 * Consecutive pieces of one string with the offset each starts at: a stable
 * React key for every non-empty piece.
 */
export function withOffsets(
  parts: string[],
): { part: string; offset: number }[] {
  let offset = 0;
  return parts.map((part) => {
    const at = offset;
    offset += part.length;
    return { offset: at, part };
  });
}
