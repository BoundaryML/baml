"use server";

// Saving BCS proposals. Each action checks the viewer on the server before it calls Convex with
// the secret, so a forged form post saves nothing. A refused save (stale, empty page, not set up)
// returns its reason with what the editor submitted, so the form shows both and nothing typed or
// uploaded is lost.
import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";
import { api } from "../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { BCS_STATUSES, type BcsStatus, bcsCall, bcsPagePath } from "@/lib/bcs-convex";
import type { BcsPageFormat } from "@/lib/bcs-render";

/** What a BCS form shows after a submission: nothing, or why it was refused and the draft. */
export type FormState = {
  error?: string;
  values?: {
    title?: string;
    status?: string;
    format?: string;
    content?: string;
    note?: string;
    slug?: string;
    parentSlug?: string;
    shepherds?: string;
    implementedBy?: string;
  };
  // The version to save over next time, after someone else saved one meanwhile.
  baseVersion?: number;
  // Changes on every refused submission, so the form re-renders with the draft.
  attempt?: number;
};

async function viewerOrFail() {
  const viewer = await currentViewer();
  if (!viewer) throw new Error("BCS proposals are for the BoundaryML team");
  return viewer;
}

function status(value: FormDataEntryValue | null): BcsStatus {
  return BCS_STATUSES.find((each) => each === value) ?? "draft";
}

/**
 * The page's content and format: an uploaded file (whose extension decides the format), else what
 * was pasted, in the format the editor picked.
 */
async function pageSource(form: FormData): Promise<{ format: BcsPageFormat; content: string }> {
  const file = form.get("file");
  if (file instanceof File && file.size > 0) {
    const format = /\.html?$/i.test(file.name)
      ? "html"
      : /\.(md|markdown)$/i.test(file.name)
        ? "markdown"
        : chosenFormat(form);
    return { format, content: await file.text() };
  }
  return { format: chosenFormat(form), content: String(form.get("content") ?? "") };
}

function chosenFormat(form: FormData): BcsPageFormat {
  return form.get("format") === "html" ? "html" : "markdown";
}

/** The version the editor's form was opened at, which the save is refused against if stale. */
function baseVersion(form: FormData): number | undefined {
  const value = Number(form.get("baseVersion"));
  return Number.isInteger(value) && value > 0 ? value : undefined;
}

function optional(value: FormDataEntryValue | null): string | undefined {
  const text = String(value ?? "").trim();
  return text || undefined;
}

/** The message for a refused save, with what to do after someone else saved meanwhile. */
function refusal(failure: { kind: string; message: string; currentVersion?: number }): string {
  return failure.kind === "stale" && failure.currentVersion
    ? `Someone saved v${failure.currentVersion} while you were editing. Your draft is kept below: ` +
        `open v${failure.currentVersion} in another tab to compare, then save again to replace it.`
    : failure.message;
}

export async function createProposal(_previous: FormState, form: FormData): Promise<FormState> {
  const viewer = await viewerOrFail();
  const title = String(form.get("title") ?? "");
  const chosen = status(form.get("status"));
  const source = await pageSource(form);
  const created = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.create, {
      secret,
      title,
      status: chosen,
      pages: [{ slug: "", ...source }],
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!created.ok) {
    return {
      error: created.failure.message,
      values: { title, status: chosen, ...source },
      attempt: Date.now(),
    };
  }
  revalidatePath("/bcs");
  redirect(`/bcs/${created.value}`);
}

/** Saves a new version with one page replaced: its content, format, and for a sub-page, parent. */
export async function savePage(
  number: number,
  slug: string,
  _previous: FormState,
  form: FormData
): Promise<FormState> {
  const viewer = await viewerOrFail();
  const source = await pageSource(form);
  const note = String(form.get("note") ?? "");
  const parentSlug = slug === "" ? undefined : optional(form.get("parentSlug"));
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      baseVersion: baseVersion(form),
      upsert: { slug, parentSlug, ...source },
      note,
    })
  );
  if (!saved.ok) {
    return {
      error: refusal(saved.failure),
      // "" keeps an explicit top-level choice, so the form shows it again.
      values: { ...source, note, parentSlug: parentSlug ?? "" },
      baseVersion: saved.failure.kind === "stale" ? saved.failure.currentVersion : undefined,
      attempt: Date.now(),
    };
  }
  revalidatePath(`/bcs/${number}`);
  redirect(bcsPagePath(number, slug));
}

/** Saves a new version with a new sub-page. */
export async function addPage(number: number, _previous: FormState, form: FormData): Promise<FormState> {
  const viewer = await viewerOrFail();
  const source = await pageSource(form);
  const slug = String(form.get("slug") ?? "").trim().toLowerCase();
  const parentSlug = optional(form.get("parentSlug"));
  const note = String(form.get("note") ?? "");
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      baseVersion: baseVersion(form),
      add: { slug, parentSlug, ...source },
      note,
    })
  );
  if (!saved.ok) {
    return {
      error: refusal(saved.failure),
      values: { ...source, note, slug, parentSlug: parentSlug ?? "" },
      baseVersion: saved.failure.kind === "stale" ? saved.failure.currentVersion : undefined,
      attempt: Date.now(),
    };
  }
  revalidatePath(`/bcs/${number}`);
  redirect(bcsPagePath(number, slug));
}

/** Saves a new version without a sub-page. */
export async function removePage(
  number: number,
  slug: string,
  _previous: FormState,
  form: FormData
): Promise<FormState> {
  const viewer = await viewerOrFail();
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      baseVersion: baseVersion(form),
      remove: slug,
      note: String(form.get("note") ?? "") || undefined,
    })
  );
  if (!saved.ok) {
    return {
      error: refusal(saved.failure),
      baseVersion: saved.failure.kind === "stale" ? saved.failure.currentVersion : undefined,
      attempt: Date.now(),
    };
  }
  revalidatePath(`/bcs/${number}`);
  redirect(bcsPagePath(number, ""));
}

/** Renames the proposal, when the editor changed the title from what the form showed. */
export async function updateProposal(
  number: number,
  _previous: FormState,
  form: FormData
): Promise<FormState> {
  const viewer = await viewerOrFail();
  const title = String(form.get("title") ?? "").trim();
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      title: title && title !== form.get("originalTitle") ? title : undefined,
    })
  );
  if (!saved.ok) return { error: saved.failure.message, values: { title }, attempt: Date.now() };
  revalidatePath(`/bcs/${number}`);
  revalidatePath("/bcs");
  return {};
}

/** Changes the proposal's status, from the header. */
export async function setStatus(number: number, _previous: FormState, form: FormData): Promise<FormState> {
  const viewer = await viewerOrFail();
  const chosen = BCS_STATUSES.find((each) => each === form.get("status"));
  if (!chosen) return { error: "Choose a status.", attempt: Date.now() };
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      status: chosen,
    })
  );
  if (!saved.ok) return { error: saved.failure.message, attempt: Date.now() };
  revalidatePath(`/bcs/${number}`);
  revalidatePath("/bcs");
  return { attempt: Date.now() };
}

function logins(value: FormDataEntryValue | null): string[] {
  return String(value ?? "")
    .split(/[\s,]+/)
    .map((each) => each.trim())
    .filter(Boolean);
}

/** Sets who shepherds the proposal, and who implemented it: GitHub logins, comma-separated. */
export async function setPeople(number: number, _previous: FormState, form: FormData): Promise<FormState> {
  const viewer = await viewerOrFail();
  const shepherds = String(form.get("shepherds") ?? "");
  const implementedBy = String(form.get("implementedBy") ?? "");
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      shepherds: logins(shepherds),
      implementedBy: form.has("implementedBy") ? logins(implementedBy) : undefined,
    })
  );
  if (!saved.ok) {
    return { error: saved.failure.message, values: { shepherds, implementedBy }, attempt: Date.now() };
  }
  revalidatePath(`/bcs/${number}`);
  revalidatePath("/bcs");
  return { attempt: Date.now() };
}

/** What a comment form shows after a submission: why it was refused, with the draft, or that it posted. */
export type CommentState = { error?: string; body?: string; posted?: number };

/** Starts a thread on a page, or on one of its blocks when the form carries one. */
export async function addComment(
  number: number,
  page: string,
  version: number,
  _previous: CommentState,
  form: FormData
): Promise<CommentState> {
  const viewer = await viewerOrFail();
  const body = String(form.get("body") ?? "");
  const block = optional(form.get("block"));
  const added = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.add, {
      secret,
      number,
      page,
      version,
      anchor: block ? { block, text: String(form.get("blockText") ?? "") } : undefined,
      type: commentType(form.get("type")),
      body,
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!added.ok) return { error: added.failure.message, body };
  revalidatePath(`/bcs/${number}`);
  return { posted: Date.now() };
}

/** Replies to the thread named by the form's commentId, written while reading `version`. */
export async function replyToComment(
  number: number,
  version: number,
  _previous: CommentState,
  form: FormData
): Promise<CommentState> {
  const viewer = await viewerOrFail();
  const body = String(form.get("body") ?? "");
  const commentId = String(form.get("commentId") ?? "");
  const replied = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.reply, {
      secret,
      number,
      commentId,
      version,
      body,
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!replied.ok) return { error: replied.failure.message, body };
  revalidatePath(`/bcs/${number}`);
  return { posted: Date.now() };
}

/** Resolves the thread named by the form's commentId, or reopens it when `resolved` is "false". */
export async function setCommentResolved(
  number: number,
  _previous: CommentState,
  form: FormData
): Promise<CommentState> {
  const viewer = await viewerOrFail();
  const commentId = String(form.get("commentId") ?? "");
  const resolved = form.get("resolved") !== "false";
  const changed = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.setResolved, {
      secret,
      number,
      commentId,
      resolved,
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!changed.ok) return { error: changed.failure.message };
  revalidatePath(`/bcs/${number}`);
  return { posted: Date.now() };
}

function commentType(value: FormDataEntryValue | null): "discussion" | "concern" | "question" {
  return value === "concern" || value === "question" ? value : "discussion";
}

/** Rewrites the viewer's own comment named by the form's commentId. */
export async function editComment(number: number, _previous: CommentState, form: FormData): Promise<CommentState> {
  const viewer = await viewerOrFail();
  const body = String(form.get("body") ?? "");
  const edited = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.edit, {
      secret,
      number,
      commentId: String(form.get("commentId") ?? ""),
      body,
      type: form.has("type") ? commentType(form.get("type")) : undefined,
      author: viewer.login,
    })
  );
  if (!edited.ok) return { error: edited.failure.message, body };
  revalidatePath(`/bcs/${number}`);
  return { posted: Date.now() };
}

/** Deletes the viewer's own comment named by the form's commentId. */
export async function deleteComment(number: number, _previous: CommentState, form: FormData): Promise<CommentState> {
  const viewer = await viewerOrFail();
  const removed = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.remove, {
      secret,
      number,
      commentId: String(form.get("commentId") ?? ""),
      author: viewer.login,
    })
  );
  if (!removed.ok) return { error: removed.failure.message };
  revalidatePath(`/bcs/${number}`);
  return { posted: Date.now() };
}

const REACTIONS = ["thumbsUp", "thumbsDown", "heart", "thinking"] as const;

/** Adds or takes back the viewer's reaction (the form's reaction) on the form's commentId. */
export async function toggleReaction(number: number, _previous: CommentState, form: FormData): Promise<CommentState> {
  const viewer = await viewerOrFail();
  const reaction = REACTIONS.find((each) => each === form.get("reaction"));
  if (!reaction) return { error: "Unknown reaction." };
  const toggled = await bcsCall((convex, secret) =>
    convex.mutation(api.bcsComments.toggleReaction, {
      secret,
      number,
      commentId: String(form.get("commentId") ?? ""),
      reaction,
      author: viewer.login,
    })
  );
  if (!toggled.ok) return { error: toggled.failure.message };
  revalidatePath(`/bcs/${number}`);
  return { posted: Date.now() };
}

/** Takes or changes the viewer's stance on a version; "clear" withdraws it. */
export async function setStance(
  number: number,
  version: number,
  _previous: FormState,
  form: FormData
): Promise<FormState> {
  const viewer = await viewerOrFail();
  const choice = form.get("stance");
  const comment = String(form.get("comment") ?? "");
  const saved =
    choice === "clear"
      ? await bcsCall((convex, secret) =>
          convex.mutation(api.bcsStances.clear, { secret, number, version, author: viewer.login })
        )
      : choice === "support" || choice === "neutral" || choice === "oppose"
        ? await bcsCall((convex, secret) =>
            convex.mutation(api.bcsStances.set, {
              secret,
              number,
              version,
              stance: choice,
              comment,
              author: viewer.login,
            })
          )
        : null;
  if (!saved) return { error: "Choose support, neutral or oppose.", attempt: Date.now() };
  if (!saved.ok) return { error: saved.failure.message, values: { note: comment }, attempt: Date.now() };
  revalidatePath(`/bcs/${number}`);
  return { attempt: Date.now() };
}
