"use client";

// Transcript comments: read over the ATB Convex websocket, write through the
// password-gated transcriptComments:create mutation (the shared password is
// checked server-side against the deployment's ATB_COMMENTS_PASSWORD).

import { makeFunctionReference } from "convex/server";
import { useMutation, useQuery } from "convex/react";

export type TranscriptComment = {
  _id: string;
  trophyId: string;
  taskId?: string | null;
  turnIndex?: number | null;
  author: string;
  body: string;
  /** Optional snippet the commenter highlighted in the transcript. */
  quote?: string | null;
  status: string;
  createdAt: number;
};

const listRef = makeFunctionReference<
  "query",
  { field: string; value: string; index: string; limit: number },
  unknown
>("transcriptComments:list");

const createRef = makeFunctionReference<
  "mutation",
  { doc: Record<string, unknown>; password: string },
  unknown
>("transcriptComments:create");

/** Live comment list for one run (newest-first from Convex; we sort oldest-first). */
export function useComments(trophyId?: string | null): TranscriptComment[] | undefined {
  const rows = useQuery(
    listRef,
    trophyId
      ? { field: "trophyId", value: trophyId, index: "by_trophy", limit: 200 }
      : "skip",
  ) as TranscriptComment[] | undefined;
  return rows ? [...rows].sort((a, b) => a.createdAt - b.createdAt) : rows;
}

export function useAddComment() {
  const mutate = useMutation(createRef);
  return async (doc: {
    trophyId: string;
    taskId?: string;
    turnIndex?: number;
    author: string;
    body: string;
  }) => {
    const password = getCommentsKey();
    if (!password) throw new Error("invalid comments password");
    try {
      await mutate({ doc, password });
    } catch (e) {
      // A rejected password clears the stored key so the form re-prompts.
      if (String(e).includes("invalid comments password")) clearCommentsKey();
      throw e;
    }
  };
}

// ---- localStorage bits (shared password + display name) ----

const KEY = "atb-comments-key";
const NAME = "atb-comments-name";

export function getCommentsKey(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem(KEY) ?? "";
}

export function setCommentsKey(v: string) {
  window.localStorage.setItem(KEY, v);
}

export function clearCommentsKey() {
  window.localStorage.removeItem(KEY);
}

export function getCommentAuthor(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem(NAME) ?? "";
}

export function setCommentAuthor(v: string) {
  window.localStorage.setItem(NAME, v);
}
