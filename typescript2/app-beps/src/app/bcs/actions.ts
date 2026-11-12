"use server";

// Saving BCS proposals. Each action checks the viewer on the server before it calls Convex with
// the secret, so a forged form post saves nothing. A refused save (stale, empty page, not set up)
// returns its reason with what the editor submitted, so the form shows both and nothing typed or
// uploaded is lost.
import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";
import { api } from "../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { BCS_STATUSES, type BcsStatus, bcsCall } from "@/lib/bcs-convex";

/** What a BCS form shows after a submission: nothing, or why it was refused and the draft. */
export type FormState = {
  error?: string;
  values?: { title?: string; status?: string; html?: string; note?: string };
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

/** The page's HTML: an uploaded file, else what was pasted. */
async function html(form: FormData): Promise<string> {
  const file = form.get("file");
  if (file instanceof File && file.size > 0) {
    return file.text();
  }
  return String(form.get("html") ?? "");
}

export async function createProposal(_previous: FormState, form: FormData): Promise<FormState> {
  const viewer = await viewerOrFail();
  const title = String(form.get("title") ?? "");
  const chosen = status(form.get("status"));
  const page = await html(form);
  const created = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.create, {
      secret,
      title,
      status: chosen,
      html: page,
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!created.ok) {
    return {
      error: created.failure.message,
      values: { title, status: chosen, html: page },
      attempt: Date.now(),
    };
  }
  revalidatePath("/bcs");
  redirect(`/bcs/${created.value}`);
}

/** The version the editor's form was opened at, which the save is refused against if stale. */
function baseVersion(form: FormData): number | undefined {
  const value = Number(form.get("baseVersion"));
  return Number.isInteger(value) && value > 0 ? value : undefined;
}

export async function saveVersion(
  number: number,
  _previous: FormState,
  form: FormData
): Promise<FormState> {
  const viewer = await viewerOrFail();
  const page = await html(form);
  const note = String(form.get("note") ?? "");
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      baseVersion: baseVersion(form),
      html: page,
      note,
    })
  );
  if (!saved.ok) {
    const { kind, message, currentVersion } = saved.failure;
    return {
      error:
        kind === "stale" && currentVersion
          ? `Someone saved v${currentVersion} while you were editing. Your draft is kept below: ` +
            `open v${currentVersion} in another tab to compare, then save again to replace it.`
          : message,
      values: { html: page, note },
      baseVersion: kind === "stale" ? currentVersion : undefined,
      attempt: Date.now(),
    };
  }
  revalidatePath(`/bcs/${number}`);
  redirect(`/bcs/${number}`);
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
