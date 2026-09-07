import type { Turn } from "@/lib/play-runs";

export function Turns({ turns }: { turns: Turn[] }) {
  return <div className="space-y-4">{turns.map((turn, index) => <article className="border rounded p-4 space-y-3" key={index}>
    <h3 className="text-sm font-semibold capitalize">{turn.role}</h3>
    {turn.content.map((block, i) => block.type === "text"
      ? <p className="whitespace-pre-wrap break-words text-sm" key={i}>{block.text}</p>
      : <details key={i} className="text-sm">
        <summary className="cursor-pointer">{block.type === "tool_use" ? `Tool: ${block.name}` : "Tool result"}</summary>
        <pre className="whitespace-pre-wrap break-words mt-2 text-xs">{block.text}</pre>
      </details>)}
  </article>)}</div>;
}
