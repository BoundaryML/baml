'use client';

import { useState } from 'react';

import { cn } from '@/lib/utils';

import CodeView from './code-view';

/** Files grouped by directory, preserving first-seen directory order. */
function groupByDir(files: Array<[string, string]>) {
  const groups = new Map<string, Array<[string, string]>>();
  for (const [path, content] of files) {
    const dir = path.includes('/')
      ? path.slice(0, path.lastIndexOf('/'))
      : '.';
    if (!groups.has(dir)) groups.set(dir, []);
    groups.get(dir)!.push([path, content]);
  }
  return groups;
}

const fileName = (path: string) => path.slice(path.lastIndexOf('/') + 1);

/**
 * IDE-style viewer for a run's created files: a directory-grouped file tree
 * on the left, and the selected file shiki-highlighted on the right under an
 * editor tab, all inside a light editor-window chrome.
 * @param files - [path, content] pairs from the trophy's filesCreated
 */
export default function FilesIde({
  files,
}: {
  files: Array<[string, string]>;
}) {
  const [active, setActive] = useState(files[0]?.[0] ?? '');
  const groups = groupByDir(files);
  const current = files.find(([p]) => p === active);
  const lines = current ? (current[1] ?? '').split('\n').length : 0;

  return (
    <div className="overflow-hidden rounded-lg border border-border shadow-[0_4px_18px_rgba(60,50,30,0.10)]">
      {/* editor window title bar */}
      <div className="flex items-center gap-2 border-b border-border bg-muted px-3.5 py-2">
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
        <span className="ml-2 font-mono text-[11px] text-muted-foreground">
          {active || 'files'}
        </span>
        <span className="ml-auto font-mono text-[11px] text-muted-foreground">
          {files.length} files
        </span>
      </div>

      <div className="flex max-h-[520px] min-h-[260px]">
        {/* file tree */}
        <div className="w-52 shrink-0 overflow-y-auto border-r border-border bg-muted py-1.5">
          {[...groups.entries()].map(([dir, members]) => (
            <div key={dir}>
              <div className="px-3 pt-2 pb-0.5 font-mono text-[10.5px] uppercase tracking-[0.05em] text-muted-foreground">
                {dir === '.' ? '/' : dir}
              </div>
              {members.map(([path]) => (
                <button
                  key={path}
                  onClick={() => setActive(path)}
                  className={cn(
                    'block w-full cursor-pointer truncate border-0 border-l-2 bg-transparent px-3 py-1 text-left font-mono text-xs',
                    path === active
                      ? 'border-l-link bg-background text-foreground'
                      : 'border-l-transparent text-muted-foreground hover:text-foreground',
                  )}
                  title={path}
                >
                  {fileName(path)}
                </button>
              ))}
            </div>
          ))}
        </div>

        {/* editor pane */}
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex items-center border-b border-border bg-muted">
            <span className="border-r border-border bg-background px-3 py-1.5 font-mono text-xs">
              {current ? fileName(current[0]) : '—'}
            </span>
            <span className="ml-auto px-3 font-mono text-[11px] text-muted-foreground">
              {lines} lines
            </span>
          </div>
          <div className="min-h-0 flex-1 overflow-auto bg-white [&_.code-hl]:m-0 [&_pre]:!rounded-none">
            {current ? (
              <CodeView path={current[0]} content={current[1]} />
            ) : (
              <p className="p-4 text-muted-foreground">no file selected.</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
