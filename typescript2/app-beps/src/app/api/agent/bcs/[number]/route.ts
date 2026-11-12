// One proposal, for `bep bcs push`: read its current page, and save a new version.
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
 * Saves `{ html?, note?, title?, status?, baseVersion? }`: a new version when html is given,
 * refused when baseVersion is not the current one, and the title or status when given.
 */
export async function PUT(request: NextRequest, { params }: Params) {
  const viewer = authenticate(request);
  if (!("login" in viewer)) return viewer;
  const number = Number((await params).number);
  const body = await request.json().catch(() => null);
  if (!Number.isInteger(number) || !body) return json({ error: "Send a JSON body." }, 400);
  const { convex, secret } = bcsConvex();
  try {
    const current = await convex.query(api.bcs.get, { secret, number });
    if (!current) return json({ error: "No such proposal." }, 404);
    if (typeof body.baseVersion === "number" && body.baseVersion !== current.currentVersion) {
      return json(
        {
          error: `The server has v${current.currentVersion}; you edited v${body.baseVersion}. Pull first.`,
          currentVersion: current.currentVersion,
        },
        409
      );
    }
    let version = current.currentVersion;
    if (typeof body.html === "string" && body.html !== current.html) {
      version = await convex.mutation(api.bcs.addVersion, {
        secret,
        number,
        html: body.html,
        note: typeof body.note === "string" ? body.note : undefined,
        author: viewer.login,
      });
    }
    const title = typeof body.title === "string" ? body.title : undefined;
    const newStatus = status(body.status);
    if (title || newStatus) {
      await convex.mutation(api.bcs.update, { secret, number, title, status: newStatus });
    }
    return json({ number, version });
  } catch (error) {
    return failure(error);
  }
}
