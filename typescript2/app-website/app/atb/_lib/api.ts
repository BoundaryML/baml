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

export const OPEN_ISSUE_STATUSES = new Set([
  "open",
  "confirmed",
  "approved",
  "fixing",
]);

/** Reader-facing lifecycle label: reported, fixing, fixed, rejected. */
export function issueStatusLabel(i: {
  status: string;
  fixSlackTs?: string | null;
}): string {
  switch (i.status) {
    case "open":
    case "confirmed":
      return i.fixSlackTs ? "fixing" : "reported";
    case "approved":
    case "fixing":
      return "fixing";
    case "closed":
      return "fixed";
    default:
      return i.status;
  }
}

/** Strip the build-ref prefix for a readable version ("0.12.1-nightly..."). */
export function bamlRefLabel(ref?: string | null): string | null {
  return ref ? ref.replace(/^baml-language-/, "") : null;
}

const FRESH_MS = 3 * 60 * 1000;
export function workerOnline(w: { lastHeartbeat: number }, now: number) {
  return now - w.lastHeartbeat < FRESH_MS;
}
