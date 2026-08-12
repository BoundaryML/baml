"use client";

// Data access. List views read a slim snapshot from /api/state (one ~200 KB
// payload, polled while a tab is open and shared by every component).
// Detail pages subscribe to full documents over the Convex websocket, so a
// running task's transcript still updates live.

import { useSyncExternalStore } from "react";
import { makeFunctionReference } from "convex/server";
import { useQuery } from "convex/react";
import type { AtbState } from "@/app/atb/_lib/types";

// ---- slim snapshot store (one poller shared across components) ----

const POLL_MS = 12_000;
let snapshot: AtbState | undefined;
let timer: ReturnType<typeof setInterval> | null = null;
const listeners = new Set<() => void>();

async function refreshSnapshot() {
  try {
    const res = await fetch("/api/atb/state", { cache: "no-store" });
    if (!res.ok) return;
    snapshot = (await res.json()) as AtbState;
    listeners.forEach((l) => l());
  } catch {
    // transient network failure: keep the last snapshot
  }
}

function subscribeSnapshot(listener: () => void) {
  listeners.add(listener);
  if (!timer) {
    void refreshSnapshot();
    timer = setInterval(() => {
      if (document.visibilityState === "visible") void refreshSnapshot();
    }, POLL_MS);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && timer) {
      clearInterval(timer);
      timer = null;
    }
  };
}

/** The slim live snapshot every list view renders from. */
export function useAtbState(): AtbState | undefined {
  return useSyncExternalStore(
    subscribeSnapshot,
    () => snapshot,
    () => undefined,
  );
}

// ---- full single-document subscriptions (detail pages) ----

const getRef = (table: string) =>
  makeFunctionReference<"query", { id: string }, unknown>(`${table}:get`);

/** Subscribe to one full row by id over the Convex websocket. */
export function useDoc<T>(
  table: string,
  id: string | null,
): T | null | undefined {
  return useQuery(getRef(table), id ? { id } : "skip") as
    | T
    | null
    | undefined;
}

// ---- shared helpers ----

// An issue is "done" only at a terminal status; everything else is still in-flight.
// Defined as the complement (terminal set) so a NEW backend status surfaces on the
// open tab instead of vanishing. The old hardcoded open-set
// ({open,confirmed,approved,fixing}) dropped every dispatching/tocursor/prprep/
// pr_ready/verifying/redraft/redrafting/needs_human issue off the default view
// (and "fixing" was never even a real backend status).
// `failed` is terminal: it's a dead-end (a give-up / queue-failure state, not an
// in-flight one), so it rests on the closed tab alongside closed/rejected rather
// than flooding the actionable "open" view (it's the single largest bucket).
export const TERMINAL_ISSUE_STATUSES = new Set(["closed", "rejected", "failed"]);

/** True while an issue is still in the pipeline (anything not terminal). */
export function isOpenIssueStatus(status: string): boolean {
  return !TERMINAL_ISSUE_STATUSES.has(status);
}

/** Reader-facing lifecycle label across the full issue lifecycle. */
export function issueStatusLabel(i: {
  status: string;
  fixSlackTs?: string | null;
}): string {
  switch (i.status) {
    case "open":
    case "confirmed":
      return i.fixSlackTs ? "fixing" : "reported";
    case "approved":
    case "dispatching":
    case "tocursor":
    case "prprep":
    case "pr_ready":
      return "fixing";
    case "verifying":
    case "reverify":
    case "reverifying":
      return "verifying";
    case "redraft":
    case "redrafting":
      return "redrafting";
    case "needs_human":
      return "needs human";
    case "failed":
      return "failed";
    case "closed":
      return "fixed";
    case "rejected":
      return "rejected";
    default:
      return i.status;
  }
}

/** Strip the build-ref prefix for a readable version ("0.12.1-nightly..."). */
export function bamlRefLabel(ref?: string | null): string | null {
  return ref ? ref.replace(/^baml-language-/, "") : null;
}

/** Short repo label for a skill URL ("BoundaryML/baml-skill" from the clone URL). */
export function skillRepoLabel(url?: string | null): string | null {
  if (!url) return null;
  return url.replace(/^https?:\/\/(www\.)?github\.com\//, "").replace(/\.git$/, "");
}

const FRESH_MS = 3 * 60 * 1000;
export function workerOnline(w: { lastHeartbeat: number }, now: number) {
  return now - w.lastHeartbeat < FRESH_MS;
}
