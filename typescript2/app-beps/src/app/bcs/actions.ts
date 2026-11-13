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

/** Saves the title and status, each only when the editor changed it from what the form showed. */
export async function updateProposal(
  number: number,
  _previous: FormState,
  form: FormData
): Promise<FormState> {
  const viewer = await viewerOrFail();
  const title = String(form.get("title") ?? "").trim();
  const newStatus = status(form.get("status"));
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      title: title && title !== form.get("originalTitle") ? title : undefined,
      status: newStatus !== form.get("originalStatus") ? newStatus : undefined,
    })
  );
  if (!saved.ok) {
    return {
      error: saved.failure.message,
      values: { title, status: newStatus },
      attempt: Date.now(),
    };
  }
  revalidatePath(`/bcs/${number}`);
  revalidatePath("/bcs");
  return {};
}
