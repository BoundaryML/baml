// One proposal, for `bep bcs pull` and `bep bcs push`: read its current page, and save it.
import { NextRequest } from "next/server";
import { api } from "../../../../../../convex/_generated/api";
import { bcsConvex } from "@/lib/bcs-convex";
import { authenticate, failure, json, status } from "../shared";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = { params: Promise<{ number: string }> };

export async function GET(request: NextRequest, { params }: Params) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const number = Number((await params).number);
  const { convex, secret } = bcsConvex();
  const proposal = Number.isInteger(number)
    ? await convex.query(api.bcs.get, { secret, number })
    : null;
  if (!proposal) return json({ error: "No such proposal." }, 404);
  return json({
    number: proposal.number,
    title: proposal.title,
    status: proposal.status,
    authors: proposal.authors,
    version: proposal.version,
    updatedAt: proposal.updatedAt,
    html: proposal.html,
  });
}

/**
 * Saves `{ html?, note?, title?, status?, baseVersion? }` in one transaction: a new version when
 * html differs from the current page, and the title or status when given, all refused (409) when
 * baseVersion is no longer the current version.
 */
export async function PUT(request: NextRequest, { params }: Params) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const number = Number((await params).number);
  const body = await request.json().catch(() => null);
  if (!Number.isInteger(number) || !body) return json({ error: "Send a JSON body." }, 400);
  const asked = status(body.status);
  if (asked && typeof asked === "object") return json(asked, 400);
  const { convex, secret } = bcsConvex();
  try {
    const version = await convex.mutation(api.bcs.save, {
      secret,
      number,
      author: viewer.login,
      baseVersion: typeof body.baseVersion === "number" ? body.baseVersion : undefined,
      html: typeof body.html === "string" ? body.html : undefined,
      note: typeof body.note === "string" ? body.note : undefined,
      title: typeof body.title === "string" ? body.title : undefined,
      status: asked,
    });
    return json({ number, version });
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    if (message.includes("Someone saved")) return json({ error: failureText(message) }, 409);
    if (message.includes("No such proposal")) return json({ error: "No such proposal." }, 404);
    return failure(error);
  }
}

function failureText(message: string): string {
  return message.match(/(Someone saved[^\n]*)/)?.[1] ?? message;
}
