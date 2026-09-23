'use client';
import { useEffect, useRef, useState } from 'react';

export type TranscriptEvent = {
  role: string;
  type: string;
  text: string;
  name: string | null;
  continuation: boolean;
};

/** Claude-style scrollback, matching the former /atb terminal's markers and disclosures. */
export function AgentTerminal({
  events,
  status,
}: {
  events: TranscriptEvent[];
  status?: string;
}) {
  const scroll = useRef<HTMLElement>(null);
  const [follow, setFollow] = useState(true);
  useEffect(() => {
    if (follow && scroll.current)
      scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [events, follow]);
  return (
    <section className="overflow-hidden rounded-xl border border-zinc-700 bg-[#141416] text-zinc-200 shadow-sm">
      <header className="flex items-center gap-2 border-b border-zinc-700 px-4 py-3 font-mono text-xs">
        <span aria-hidden className="h-2.5 w-2.5 rounded-full bg-rose-400" />
        <span aria-hidden className="h-2.5 w-2.5 rounded-full bg-amber-400" />
        <span aria-hidden className="h-2.5 w-2.5 rounded-full bg-emerald-400" />
        <span className="ml-2">Claude · {status ?? 'connecting'}</span>
        <button
          aria-pressed={follow}
          className="ml-auto rounded border border-zinc-600 px-2 py-1 hover:bg-zinc-800"
          onClick={() => setFollow((v) => !v)}
          type="button"
        >
          {follow ? 'Following live' : 'Follow live'}
        </button>
      </header>
      <section
        aria-label="Agent transcript"
        className="max-h-[75vh] min-h-64 overflow-auto p-4 font-mono text-xs leading-6"
        onScroll={() => {
          const el = scroll.current;
          if (el && el.scrollHeight - el.scrollTop - el.clientHeight > 100)
            setFollow(false);
        }}
        ref={scroll}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: Keyboard users need to focus the scrollback to scroll it.
        tabIndex={0}
      >
        {!events.length && (
          <p className="text-zinc-400">Waiting for the first message…</p>
        )}
        {events.map((event, i) => {
          const tool = event.type !== 'text';
          const marker = tool
            ? event.type === 'tool_use'
              ? '⏺'
              : '⎿'
            : event.role === 'user'
              ? '>'
              : '⏺';
          const color = tool
            ? 'text-amber-300'
            : event.role === 'user'
              ? 'text-sky-300'
              : 'text-emerald-300';
          const lines = event.text.split('\n');
          return (
            // biome-ignore lint/suspicious/noArrayIndexKey: Transcript entries are append-only; position preserves open disclosures as text streams.
            <article className="mb-4 last:mb-0" key={i}>
              <div className={color}>
                {marker} {event.name ?? (tool ? 'Tool result' : event.role)}
                {event.continuation ? ' (continued)' : ''}
              </div>
              {tool || lines.length > 18 ? (
                <details className="ml-4">
                  <summary className="cursor-pointer text-zinc-400">
                    {tool ? 'Input / output' : 'Message'} · {lines.length} lines
                  </summary>
                  <pre className="whitespace-pre-wrap break-words">
                    {event.text}
                  </pre>
                </details>
              ) : (
                <pre className="ml-4 whitespace-pre-wrap break-words">
                  {event.text}
                </pre>
              )}
            </article>
          );
        })}
      </section>
    </section>
  );
}
