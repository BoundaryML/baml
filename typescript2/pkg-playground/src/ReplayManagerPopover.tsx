import React, { useEffect, useMemo, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "./components/ui/dialog";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./components/ui/collapsible";
import { ChevronRight, RotateCcw, Circle, CheckCircle2, Globe } from "lucide-react";
import { cn } from "./lib/utils";
import { Switch } from "./components/ui/switch";
import type { ReplayGroup } from "./worker-protocol";

interface ReplayManagerDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  entries: ReplayGroup[];
  onToggleReplay: (fetchId: number, pinned: boolean) => void;
  onRequestState: () => void;
}

const METHOD_COLORS: Record<string, string> = {
  GET: "bg-emerald-500/15 text-emerald-400 border-emerald-500/30",
  POST: "bg-blue-500/15 text-blue-400 border-blue-500/30",
  PUT: "bg-amber-500/15 text-amber-400 border-amber-500/30",
  DELETE: "bg-red-500/15 text-red-400 border-red-500/30",
  PATCH: "bg-purple-500/15 text-purple-400 border-purple-500/30",
};

function MethodBadge({ method }: { method: string }) {
  return (
    <span className={cn("text-[9px] font-bold font-mono px-1.5 py-0.5 rounded border", METHOD_COLORS[method] ?? "bg-zinc-500/15 text-zinc-400 border-zinc-500/30")}>
      {method}
    </span>
  );
}

function StatusBadge({ status }: { status: number }) {
  return (
    <span className={cn("text-[10px] font-mono font-semibold px-1.5 py-0.5 rounded", status >= 400 ? "bg-red-500/15 text-red-400" : "bg-emerald-500/15 text-emerald-400")}>
      {status}
    </span>
  );
}

function trim(s: string, len: number): string {
  if (!s) return "(empty)";
  if (s.length <= len) return s;
  return s.slice(0, len) + "…";
}

function tryPrettyJson(s: string): string {
  try { return JSON.stringify(JSON.parse(s), null, 2); } catch { return s; }
}

function timeAgo(epochSecs: number): string {
  if (!epochSecs) return "";
  const delta = Math.floor(Date.now() / 1000) - epochSecs;
  if (delta < 5) return "just now";
  if (delta < 60) return `${delta}s ago`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

function parseUrl(url: string): { domain: string; path: string } {
  try { const u = new URL(url); return { domain: u.host, path: u.pathname }; }
  catch { return { domain: url, path: "/" }; }
}

// ── Tree data model ────────────────────────────────────────────

interface EndpointNode {
  method: string;
  url: string;
  path: string;
  requests: ReplayGroup[];
  activeCount: number;
}

interface DomainNode {
  domain: string;
  endpoints: EndpointNode[];
  activeCount: number;
}

function buildTree(entries: ReplayGroup[]): DomainNode[] {
  const endpointMap = new Map<string, EndpointNode>();
  for (const g of entries) {
    const epKey = `${g.display.method} ${g.display.url}`;
    let ep = endpointMap.get(epKey);
    if (!ep) {
      const { path } = parseUrl(g.display.url);
      ep = { method: g.display.method, url: g.display.url, path, requests: [], activeCount: 0 };
      endpointMap.set(epKey, ep);
    }
    ep.requests.push(g);
    if (g.pinnedFetchId != null) ep.activeCount++;
  }
  const domainMap = new Map<string, EndpointNode[]>();
  for (const ep of endpointMap.values()) {
    const { domain } = parseUrl(ep.url);
    const list = domainMap.get(domain);
    if (list) list.push(ep); else domainMap.set(domain, [ep]);
  }
  return Array.from(domainMap.entries()).map(([domain, endpoints]) => ({
    domain, endpoints, activeCount: endpoints.reduce((n, ep) => n + ep.activeCount, 0),
  }));
}

// ── Left panel: nav tree (domain → endpoint → request) ─────────

function RequestNavItem({
  group,
  isActive,
  onSelect,
}: {
  group: ReplayGroup;
  isActive: boolean;
  onSelect: () => void;
}) {
  const hasPinned = group.pinnedFetchId != null;
  return (
    <button
      onClick={onSelect}
      className={cn(
        "flex items-center gap-1.5 w-full py-1 pl-8 pr-2 rounded text-left text-[10px] cursor-pointer",
        isActive ? "bg-[#1e1528] ring-1 ring-[#7c3aed]/50 text-vsc-text" : "text-vsc-text-muted hover:bg-vsc-hover",
      )}
    >
      {hasPinned ? (
        <CheckCircle2 size={10} className="text-[#7c3aed] shrink-0" />
      ) : (
        <Circle size={10} className="text-vsc-text-faint shrink-0" />
      )}
      <span className="truncate">{trim(group.display.body, 40)}</span>
      <span className="text-[9px] text-vsc-text-faint shrink-0 ml-auto tabular-nums">
        {group.recordings.length}
      </span>
    </button>
  );
}

function EndpointNavNode({
  endpoint,
  selectedGroupKey,
  onSelectGroup,
}: {
  endpoint: EndpointNode;
  selectedGroupKey: string | null;
  onSelectGroup: (group: ReplayGroup) => void;
}) {
  return (
    <div>
      <div className="flex items-center gap-1.5 py-1 pl-4 pr-2">
        <MethodBadge method={endpoint.method} />
        <span className="text-[11px] text-vsc-text font-mono truncate" title={endpoint.url}>{endpoint.path}</span>
        {endpoint.activeCount > 0 && (
          <span className="text-[9px] font-semibold text-[#7c3aed] bg-[#7c3aed]/15 px-1.5 py-0.5 rounded-full shrink-0 ml-auto">{endpoint.activeCount}</span>
        )}
      </div>
      {endpoint.requests.map((group) => (
        <RequestNavItem
          key={group.key}
          group={group}
          isActive={group.key === selectedGroupKey}
          onSelect={() => onSelectGroup(group)}
        />
      ))}
    </div>
  );
}

function DomainNavNode({
  node,
  selectedGroupKey,
  onSelectGroup,
  defaultOpen,
}: {
  node: DomainNode;
  selectedGroupKey: string | null;
  onSelectGroup: (group: ReplayGroup) => void;
  defaultOpen: boolean;
}) {
  const [expanded, setExpanded] = useState(defaultOpen);
  return (
    <Collapsible open={expanded} onOpenChange={setExpanded}>
      <CollapsibleTrigger className="flex items-center gap-2 w-full px-2 py-1.5 hover:bg-vsc-hover rounded cursor-pointer">
        <ChevronRight className={cn("h-3.5 w-3.5 shrink-0 text-vsc-text-faint transition-transform", expanded && "rotate-90")} />
        <Globe size={12} className="shrink-0 text-vsc-text-muted" />
        <span className="text-[11px] font-medium text-vsc-text truncate">{node.domain}</span>
        {node.activeCount > 0 && (
          <span className="text-[9px] font-semibold text-[#7c3aed] bg-[#7c3aed]/15 px-1.5 py-0.5 rounded-full shrink-0 ml-auto">{node.activeCount} active</span>
        )}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pb-1">
          {node.endpoints.map((ep) => (
            <EndpointNavNode key={`${ep.method} ${ep.url}`} endpoint={ep} selectedGroupKey={selectedGroupKey} onSelectGroup={onSelectGroup} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

// ── Right panel: selected request's responses + detail ──────────

function RightPanel({
  group,
  onToggleReplay,
}: {
  group: ReplayGroup;
  onToggleReplay: (fetchId: number, pinned: boolean) => void;
}) {
  const [selectedRecFetchId, setSelectedRecFetchId] = useState<number | null>(null);
  const [activeTab, setActiveTab] = useState<"request" | "response">("request");
  const isEnabled = group.pinnedFetchId != null;

  const effectiveSelectedId = selectedRecFetchId
    ?? group.pinnedFetchId
    ?? group.recordings[0]?.fetchId
    ?? null;

  const selectedRec = group.recordings.find((r) => r.fetchId === effectiveSelectedId) ?? null;

  const handleToggle = (checked: boolean) => {
    if (!checked) {
      if (group.pinnedFetchId != null) onToggleReplay(group.pinnedFetchId, false);
    } else if (effectiveSelectedId != null) {
      onToggleReplay(effectiveSelectedId, true);
    }
  };

  const handleSelectAndPin = (fetchId: number) => {
    setSelectedRecFetchId(fetchId);
    if (isEnabled) onToggleReplay(fetchId, true);
  };

  return (
    <div className="flex flex-col flex-1 min-h-0 min-w-0">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-vsc-border shrink-0">
        <MethodBadge method={group.display.method} />
        <span className="text-[11px] font-mono text-vsc-text truncate" title={group.display.url}>
          {parseUrl(group.display.url).path}
        </span>
        <div className="ml-auto flex items-center gap-2 shrink-0">
          <span className={cn("text-[10px] font-medium", isEnabled ? "text-[#7c3aed]" : "text-vsc-text-faint")}>
            {isEnabled ? "Replaying" : "Replay off"}
          </span>
          <Switch checked={isEnabled} onCheckedChange={handleToggle} size="sm" />
        </div>
      </div>

      {/* Response list */}
      <div className="shrink-0 border-b border-vsc-border">
        <div className="px-3 py-1.5">
          <span className="text-[9px] font-semibold text-vsc-text-faint uppercase tracking-wider">
            Responses ({group.recordings.length})
          </span>
        </div>
        <div className="px-2 pb-2 space-y-0.5 max-h-[120px] overflow-auto">
          {group.recordings.map((rec) => {
            const isActive = rec.fetchId === effectiveSelectedId;
            const isPinned = rec.fetchId === group.pinnedFetchId;
            return (
              <button
                key={rec.fetchId}
                onClick={() => handleSelectAndPin(rec.fetchId)}
                className={cn(
                  "flex items-center gap-2 w-full px-2 py-1 rounded text-left cursor-pointer",
                  isActive ? "bg-[#1e1528] ring-1 ring-[#7c3aed]/50" : "hover:bg-vsc-hover",
                )}
              >
                {isPinned ? (
                  <CheckCircle2 size={12} className="text-[#7c3aed] shrink-0" />
                ) : (
                  <Circle size={12} className="text-vsc-text-faint shrink-0" />
                )}
                <StatusBadge status={rec.status} />
                <span className="text-[10px] text-vsc-text-muted truncate flex-1">
                  {trim(rec.body, 40)}
                </span>
                {rec.recordedAt > 0 && (
                  <span className="text-[9px] text-vsc-text-faint shrink-0">{timeAgo(rec.recordedAt)}</span>
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* Simple tab bar (no Radix) */}
      <div className="flex gap-0 border-b border-vsc-border shrink-0 mx-3 mt-1">
        {(["request", "response"] as const).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={cn(
              "px-3 py-1.5 text-[11px] font-mono border-b-2 cursor-pointer",
              activeTab === tab
                ? "border-vsc-accent text-vsc-text font-semibold"
                : "border-transparent text-vsc-text-faint hover:text-vsc-text",
            )}
          >
            {tab === "request" ? "Request Body" : "Response Body"}
          </button>
        ))}
      </div>

      {/* Body content */}
      <div className="flex-1 min-h-0 px-3 pb-3 pt-1">
        <pre className="text-[10px] font-mono text-vsc-text bg-vsc-background rounded p-2 overflow-auto h-full whitespace-pre-wrap break-all border border-vsc-border">
          {activeTab === "request"
            ? tryPrettyJson(group.display.body)
            : selectedRec ? tryPrettyJson(selectedRec.body) : "(no response selected)"}
        </pre>
      </div>
    </div>
  );
}

// ── Main dialog ────────────────────────────────────────────────

export function ReplayManagerDialog({
  open,
  onOpenChange,
  entries,
  onToggleReplay,
  onRequestState,
}: ReplayManagerDialogProps) {
  const [selectedGroupKey, setSelectedGroupKey] = useState<string | null>(null);

  // Request fresh state only when dialog opens (not on every render).
  const onRequestStateRef = React.useRef(onRequestState);
  onRequestStateRef.current = onRequestState;
  useEffect(() => {
    if (open) {
      onRequestStateRef.current();
      setSelectedGroupKey(null);
    }
  }, [open]);

  const domainNodes = useMemo(() => buildTree(entries), [entries]);

  // Find the currently selected group from entries
  const selectedGroup = selectedGroupKey
    ? entries.find((g) => g.key === selectedGroupKey) ?? null
    : null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="w-[calc(100vw-2rem)] max-w-[900px] h-[70vh] !flex !flex-col overflow-hidden">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <RotateCcw size={14} className="text-vsc-text-muted" />
            Record / Replay
          </DialogTitle>
        </DialogHeader>
        <p className="text-[11px] text-vsc-text-faint px-4 -mt-2 leading-relaxed">
          HTTP calls are recorded automatically. Select a request to manage its cached responses.
        </p>
        <div className="flex flex-1 min-h-0 pb-4">
          {/* Left: nav tree */}
          <div className="w-[300px] flex-shrink-0 overflow-auto py-2 px-2 space-y-1 border-r border-vsc-border">
            {entries.length === 0 ? (
              <div className="py-8 text-xs text-vsc-text-faint text-center">
                No recorded requests yet.
                <br />
                Run a function to start recording.
              </div>
            ) : (
              domainNodes.map((node) => (
                <DomainNavNode
                  key={node.domain}
                  node={node}
                  selectedGroupKey={selectedGroupKey}
                  onSelectGroup={(g) => setSelectedGroupKey(g.key)}
                  defaultOpen={domainNodes.length <= 3}
                />
              ))
            )}
          </div>
          {/* Right: selected request detail */}
          {selectedGroup ? (
            <RightPanel group={selectedGroup} onToggleReplay={onToggleReplay} />
          ) : (
            <div className="flex-1 flex items-center justify-center">
              <p className="text-xs text-vsc-text-faint">Select a request to view responses</p>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
