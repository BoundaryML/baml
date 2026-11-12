"use server";

// Saving BCS proposals. Each action checks the viewer on the server before it calls Convex with
// the secret, so a forged form post saves nothing.
import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";
import { api } from "../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { BCS_STATUSES, type BcsStatus, bcsConvex } from "@/lib/bcs-convex";

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

export async function createProposal(form: FormData) {
  const viewer = await viewerOrFail();
  const { convex, secret } = bcsConvex();
  const number = await convex.mutation(api.bcs.create, {
    secret,
    title: String(form.get("title") ?? ""),
    status: status(form.get("status")),
    html: await html(form),
    author: viewer.login,
  });
  revalidatePath("/bcs");
  redirect(`/bcs/${number}`);
}

export async function saveVersion(number: number, form: FormData) {
  const viewer = await viewerOrFail();
  const { convex, secret } = bcsConvex();
  await convex.mutation(api.bcs.addVersion, {
    secret,
    number,
    html: await html(form),
    note: String(form.get("note") ?? ""),
    author: viewer.login,
  });
  revalidatePath(`/bcs/${number}`);
  redirect(`/bcs/${number}`);
}

export async function updateProposal(number: number, form: FormData) {
  await viewerOrFail();
  const { convex, secret } = bcsConvex();
  await convex.mutation(api.bcs.update, {
    secret,
    number,
    title: String(form.get("title") ?? "") || undefined,
    status: status(form.get("status")),
  });
  revalidatePath(`/bcs/${number}`);
  revalidatePath("/bcs");
}
