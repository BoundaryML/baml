// The `bep bcs` CLI's routes: pull every proposal, and create one.
import { NextRequest } from "next/server";
import { api } from "../../../../../convex/_generated/api";
import { bcsConvex } from "@/lib/bcs-convex";
import { authenticate, failure, json, status } from "./shared";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

/** Every proposal with its current page, for `bep bcs pull`. */
export async function GET(request: NextRequest) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const { convex, secret } = bcsConvex();
  const listed = await convex.query(api.bcs.list, { secret });
  const proposals = await Promise.all(
    listed.map(async (each) => {
      const proposal = await convex.query(api.bcs.get, { secret, number: each.number });
      return proposal && {
        number: proposal.number,
        title: proposal.title,
        status: proposal.status,
        authors: proposal.authors,
        version: proposal.version,
        updatedAt: proposal.updatedAt,
        html: proposal.html,
      };
    })
  );
  return json({ proposals: proposals.filter(Boolean) });
}

/** Creates a proposal: `{ title, html, status? }`. */
export async function POST(request: NextRequest) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const body = await request.json().catch(() => null);
  if (!body || typeof body.title !== "string" || typeof body.html !== "string") {
    return json({ error: "Send { title, html } and optionally status." }, 400);
  }
  const { convex, secret } = bcsConvex();
  try {
    const number = await convex.mutation(api.bcs.create, {
      secret,
      title: body.title,
      html: body.html,
      status: status(body.status) ?? "draft",
      author: viewer.login,
    });
    return json({ number, version: 1 }, 201);
  } catch (error) {
    return failure(error);
  }
}
