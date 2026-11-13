// The `bep bcs` CLI's routes: list every proposal, and create one.
import { NextRequest } from "next/server";
import { api } from "../../../../../convex/_generated/api";
import { bcsCall } from "@/lib/bcs-convex";
import { authenticate, json, pages, refused, status } from "./shared";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

/**
 * Every proposal's metadata, for `bep bcs pull`, which then reads each page with
 * GET /api/agent/bcs/{number}: all pages in one answer could pass Vercel's 4.5 MB limit.
 */
export async function GET(request: NextRequest) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const listed = await bcsCall((convex, secret) => convex.query(api.bcs.list, { secret }));
  if (!listed.ok) return refused(listed.failure);
  return json({
    proposals: listed.value.map((each) => ({
      number: each.number,
      title: each.title,
      status: each.status,
      authors: each.authors,
      version: each.currentVersion,
      updatedAt: each.updatedAt,
    })),
  });
}

/**
 * Creates a proposal: `{ title, pages, status? }`, where pages holds the main page (slug "") and
 * any sub-pages; `{ title, html }` (an HTML main page) still works for older `bep` scripts.
 */
export async function POST(request: NextRequest) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const body = await request.json().catch(() => null);
  if (!body || typeof body.title !== "string") {
    return json({ error: "Send { title, pages } and optionally status." }, 400);
  }
  const asked = status(body.status);
  if (asked && typeof asked === "object") return json(asked, 400);
  const sent = pages(body.pages);
  if (sent && !Array.isArray(sent)) return json(sent, 400);
  if (!sent && typeof body.html !== "string") {
    return json({ error: "Send { title, pages } and optionally status." }, 400);
  }
  const created = await bcsCall((convex, secret) =>
    convex.mutation(api.bcs.create, {
      secret,
      title: body.title,
      pages: sent,
      html: sent ? undefined : body.html,
      status: asked ?? "draft",
      author: viewer.login,
      authorGithubId: viewer.githubId,
    })
  );
  if (!created.ok) return refused(created.failure);
  return json({ number: created.value, version: 1 }, 201);
}
