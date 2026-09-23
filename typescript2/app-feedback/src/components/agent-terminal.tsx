"use client";
import { useEffect, useRef, useState } from "react";

export type TranscriptEvent = { role: string; type: string; text: string; name: string | null; continuation: boolean };

/** Claude-style scrollback, matching the former /atb terminal's markers and disclosures. */
export function AgentTerminal({ events, status }: { events: TranscriptEvent[]; status?: string }) {
  const scroll = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  useEffect(() => {
    if (follow && scroll.current) scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [events, follow]);
  return <section className="overflow-hidden rounded-xl border border-zinc-700 bg-[#141416] text-zinc-200 shadow-sm">
    <header className="flex items-center gap-2 border-b border-zinc-700 px-4 py-3 font-mono text-xs">
      <span className="h-2.5 w-2.5 rounded-full bg-rose-400" aria-hidden />
      <span className="h-2.5 w-2.5 rounded-full bg-amber-400" aria-hidden />
      <span className="h-2.5 w-2.5 rounded-full bg-emerald-400" aria-hidden />
      <span className="ml-2">Claude · {status ?? "connecting"}</span>
      <button className="ml-auto rounded border border-zinc-600 px-2 py-1 hover:bg-zinc-800" onClick={() => setFollow(v => !v)} aria-pressed={follow}>{follow ? "Following live" : "Follow live"}</button>
    </header>
    <div ref={scroll} role="region" aria-label="Agent transcript" tabIndex={0} className="max-h-[75vh] min-h-64 overflow-auto p-4 font-mono text-xs leading-6" onScroll={() => {
      const el = scroll.current;
      if (el && el.scrollHeight - el.scrollTop - el.clientHeight > 100) setFollow(false);
    }}>
      {!events.length && <p className="text-zinc-400">Waiting for the first message…</p>}
      {events.map((event, i) => {
        const tool = event.type !== "text";
        const marker = tool ? event.type === "tool_use" ? "⏺" : "⎿" : event.role === "user" ? ">" : "⏺";
        const color = tool ? "text-amber-300" : event.role === "user" ? "text-sky-300" : "text-emerald-300";
        const lines = event.text.split("\n");
        return <article key={i} className="mb-4 last:mb-0">
          <div className={color}>{marker} {event.name ?? (tool ? "Tool result" : event.role)}{event.continuation ? " (continued)" : ""}</div>
          {tool || lines.length > 18 ? <details className="ml-4"><summary className="cursor-pointer text-zinc-400">{tool ? "Input / output" : "Message"} · {lines.length} lines</summary><pre className="whitespace-pre-wrap break-words">{event.text}</pre></details> : <pre className="ml-4 whitespace-pre-wrap break-words">{event.text}</pre>}
        </article>;
      })}
    </div>
  </section>;
}
