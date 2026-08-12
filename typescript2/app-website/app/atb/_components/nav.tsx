"use client";

// Section links as a vertical rail beside the content (sticky under the
// site navbar). Collapses to a slim horizontal row on small screens.

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useAtbState, workerOnline } from "@/app/atb/_lib/api";
import { useNow } from "@/app/atb/_components/use-now";

const LINKS = [
  { href: "/atb", label: "Feed" },
  { href: "/atb/runs", label: "Runs" },
  { href: "/atb/agents", label: "Agents" },
  { href: "/atb/issues", label: "Issues" },
  { href: "/atb/arena", label: "Arena" },
  { href: "/atb/builds", label: "Builds" },
];

function useLive() {
  const now = useNow();
  const state = useAtbState();
  return {
    online: (state?.workers ?? []).filter((w) => workerOnline(w, now)).length,
    running: (state?.tasks ?? []).filter((t) => t.status === "running").length,
  };
}

function isActive(pathname: string, href: string) {
  return href === "/atb" ? pathname === "/atb" : pathname.startsWith(href);
}

function OnlineDot({ online, running }: { online: number; running: number }) {
  return (
    <span className="flex flex-col gap-1.5 text-xs text-atb-ink-3">
      {running > 0 && (
        <span className="flex items-center gap-1.5 text-atb-accent-deep">
          <span className="relative inline-flex w-1.5 h-1.5 rounded-full bg-atb-accent atb-pulse-ring text-atb-accent" />
          {running} running
        </span>
      )}
      <span className="flex items-center gap-1.5">
        <span
          className={`inline-flex w-1.5 h-1.5 rounded-full ${
            online > 0 ? "bg-atb-olive" : "bg-atb-line-strong"
          }`}
        />
        {online} agents online
      </span>
    </span>
  );
}

/** Vertical rail for md+ screens; rendered inside the layout's sidebar. */
export function NavRail() {
  const pathname = usePathname();
  const { online, running } = useLive();
  return (
    <nav className="flex flex-col gap-1 text-sm">
      {LINKS.map((l) => {
        const active = isActive(pathname, l.href);
        return (
          <Link
            key={l.href}
            href={l.href}
            className={`rounded-lg px-3 py-1.5 -ml-3 transition-colors ${
              active
                ? "text-atb-ink font-medium bg-atb-ivory border-l-2 border-atb-accent rounded-l-none"
                : "text-atb-ink-3 hover:text-atb-ink-2"
            }`}
          >
            {l.label}
          </Link>
        );
      })}
      <div className="mt-5 pl-0.5">
        <OnlineDot online={online} running={running} />
      </div>
    </nav>
  );
}

/** Compact horizontal strip for small screens. */
export function NavStrip() {
  const pathname = usePathname();
  const { online, running } = useLive();
  return (
    <div className="flex items-center gap-4 text-sm overflow-x-auto">
      {LINKS.map((l) => {
        const active = isActive(pathname, l.href);
        return (
          <Link
            key={l.href}
            href={l.href}
            className={`whitespace-nowrap transition-colors ${
              active
                ? "text-atb-ink font-medium underline underline-offset-8 decoration-atb-accent decoration-2"
                : "text-atb-ink-3 hover:text-atb-ink-2"
            }`}
          >
            {l.label}
          </Link>
        );
      })}
      <span className="ml-auto shrink-0 hidden xs:block">
        <OnlineDot online={online} running={running} />
      </span>
    </div>
  );
}
