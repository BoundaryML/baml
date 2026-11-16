import { NextRequest, NextResponse } from "next/server";
import { api } from "../../../../../convex/_generated/api";
import { currentViewer } from "@/lib/bcs-auth";
import { bcsCall, bcsPagePath } from "@/lib/bcs-convex";
import { renderBcsPage } from "@/lib/bcs-render";

export const dynamic = "force-dynamic";

/**
 * A proposal's page on its own, for reading full screen. The CSP sandbox gives it an opaque
 * origin, as the frame does, so its scripts cannot read this site or its cookies.
 */
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ number: string }> }
) {
  if (!(await currentViewer())) {
    return new NextResponse("BCS proposals are for the BoundaryML team.", { status: 404 });
  }
  const number = Number((await params).number);
  const asked = Number(request.nextUrl.searchParams.get("v"));
  const slug = request.nextUrl.searchParams.get("page") ?? "";
  if (!Number.isInteger(number)) return new NextResponse("Not found", { status: 404 });
  const found = await bcsCall((convex, secret) =>
    convex.query(api.bcs.get, {
      secret,
      number,
      version: Number.isInteger(asked) && asked > 0 ? asked : undefined,
      page: slug || undefined,
    })
  );
  if (!found.ok) return new NextResponse(found.failure.message, { status: 503 });
  const proposal = found.value;
  if (!proposal) return new NextResponse("Not found", { status: 404 });
  const old = proposal.version !== proposal.currentVersion;
  const { document } = await renderBcsPage({
    format: proposal.page.format,
    content: proposal.page.content,
    title: slug === "" ? proposal.title : `${proposal.page.title} · ${proposal.title}`,
    pageHref: (to) => bcsPagePath(proposal.number, to, old ? proposal.version : undefined),
  });
  return new NextResponse(document, {
    headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Content-Security-Policy": "sandbox allow-scripts allow-popups",
      "Cache-Control": "private, no-store",
      "X-Robots-Tag": "noindex",
    },
  });
}
