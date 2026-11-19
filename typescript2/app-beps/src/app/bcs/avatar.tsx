"use client";

// A member's GitHub picture, from their login: GitHub serves every account's at github.com/<login>.png.
// If it cannot load, the member's initial in a colored circle, as BEP comments show authors.
import { useEffect, useRef, useState } from "react";
import { Bot } from "lucide-react";
import { KNOWN_AGENTS, agentName } from "../../../convex/bcsAgents";

const COLORS = [
  "bg-blue-500",
  "bg-green-500",
  "bg-purple-500",
  "bg-pink-500",
  "bg-indigo-500",
  "bg-teal-500",
  "bg-orange-500",
  "bg-cyan-500",
];

const SIZES = { xs: 16, sm: 20, md: 24 } as const;
type Size = keyof typeof SIZES;

export function GitHubAvatar({ login, size = "md" }: { login: string; size?: Size }) {
  const [failed, setFailed] = useState(false);
  const image = useRef<HTMLImageElement>(null);
  // An image that failed before React attached onError (during hydration) reports nothing: check.
  useEffect(() => {
    const element = image.current;
    if (element?.complete && element.naturalWidth === 0) setFailed(true);
  }, []);
  const pixels = SIZES[size];
  const style = { width: pixels, height: pixels };
  if (failed || !/^[A-Za-z0-9-]+$/.test(login)) {
    const color = COLORS[[...login].reduce((sum, char) => sum + char.charCodeAt(0), 0) % COLORS.length];
    return (
      <span
        aria-hidden
        style={{ ...style, fontSize: pixels * 0.5 }}
        className={`inline-flex shrink-0 items-center justify-center rounded-full font-medium text-white ${color}`}
      >
        {login[0]?.toUpperCase() ?? "?"}
      </span>
    );
  }
  return (
    // A plain image: GitHub resizes it, so Next's image optimizer would only add a hop.
    // eslint-disable-next-line @next/next/no-img-element
    <img
      ref={image}
      src={`https://github.com/${login}.png?size=${pixels * 2}`}
      alt=""
      aria-hidden
      loading="lazy"
      style={style}
      onError={() => setFailed(true)}
      className="shrink-0 rounded-full bg-muted"
    />
  );
}

/**
 * Who wrote a comment: the member's picture and login. A comment an agent wrote for the member (from
 * the agent routes, never the browser or the plain CLI) also shows the agent's picture overlapping
 * the member's, as GitHub shows a co-author, and a label naming the agent.
 */
export function CommentAuthor({ login, agent, size = "md" }: { login: string; agent?: string; size?: Size }) {
  const name = <span className="font-medium text-foreground">{login}</span>;
  if (!agent) {
    return (
      <span className="inline-flex items-center gap-1.5">
        <GitHubAvatar login={login} size={size} />
        {name}
      </span>
    );
  }
  const known = KNOWN_AGENTS[agent];
  const pixels = SIZES[size];
  const posted = `Posted by @${login}'s agent, ${agentName(agent)}`;
  return (
    <span className="inline-flex items-center gap-1.5" title={posted}>
      <span className="inline-flex items-center">
        <GitHubAvatar login={login} size={size} />
        <span className="-ml-1.5 inline-flex rounded-full ring-2 ring-background">
          {known ? (
            <GitHubAvatar login={known.github} size={size} />
          ) : (
            <span
              aria-hidden
              style={{ width: pixels, height: pixels }}
              className="inline-flex items-center justify-center rounded-full bg-muted text-muted-foreground"
            >
              <Bot style={{ width: pixels * 0.6, height: pixels * 0.6 }} />
            </span>
          )}
        </span>
      </span>
      {name}
      <span className="inline-flex items-center gap-1 rounded-full border px-1.5 py-px text-[11px] leading-4">
        <Bot aria-hidden className="h-3 w-3" />
        <span className="sr-only">{posted}</span>
        <span aria-hidden>via {agentName(agent)}</span>
      </span>
    </span>
  );
}
