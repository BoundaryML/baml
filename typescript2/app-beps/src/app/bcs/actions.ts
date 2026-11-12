"use server";

// Saving BCS proposals. Each action checks the viewer on the server before it calls Convex with
// the secret, so a forged form post saves nothing. A refused save (stale, empty page, not set up)
// returns the editor to the page with the reason, rather than to an error screen.
import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";
import { api } from "../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { BCS_STATUSES, type BcsStatus, bcsCall } from "@/lib/bcs-convex";

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

function withError(path: string, message: string): string {
  return `${path}?error=${encodeURIComponent(message)}`;
}

export async function createProposal(form: FormData) {
  const viewer = await viewerOrFail();
  const page = await html(form);
  const created = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.create, {
      secret,
      title: String(form.get("title") ?? ""),
      status: status(form.get("status")),
      html: page,
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!created.ok) redirect(withError("/bcs/new", created.failure.message));
  revalidatePath("/bcs");
  redirect(`/bcs/${created.value}`);
}

/** The version the editor's form was opened at, which the save is refused against if stale. */
function baseVersion(form: FormData): number | undefined {
  const value = Number(form.get("baseVersion"));
  return Number.isInteger(value) && value > 0 ? value : undefined;
}

export async function saveVersion(number: number, form: FormData) {
  const viewer = await viewerOrFail();
  const page = await html(form);
  const saved = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      authorGithubId: viewer.githubId,
      baseVersion: baseVersion(form),
      html: page,
      note: String(form.get("note") ?? ""),
    })
  );
  if (!saved.ok) redirect(withError(`/bcs/${number}`, saved.failure.message));
  revalidatePath(`/bcs/${number}`);
  redirect(`/bcs/${number}`);
}

/** Saves the title and status, each only when the editor changed it from what the form showed. */
export async function updateProposal(number: number, form: FormData) {
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
  if (!saved.ok) redirect(withError(`/bcs/${number}`, saved.failure.message));
  revalidatePath(`/bcs/${number}`);
  revalidatePath("/bcs");
}
