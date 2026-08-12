import { NextRequest, NextResponse } from "next/server";

// Server-side proxy to the bammy-service blob store (ATB_SERVICE_URL): raw Claude Code
// transcripts and skill snapshots live on its volume, bearer-protected.
// The token never reaches the browser.

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ key: string[] }> },
) {
  const base = process.env.ATB_SERVICE_URL;
  const token = process.env.ATB_SERVICE_TOKEN;
  if (!base || !token) {
    return new NextResponse("transcript service not configured", {
      status: 503,
    });
  }
  const { key } = await params;
  const path = key.map(encodeURIComponent).join("/");
  const res = await fetch(
    `${base.replace(/\/$/, "")}/transcripts/${path}`,
    { headers: { Authorization: `Bearer ${token}` }, cache: "no-store" },
  );
  if (!res.ok) {
    return new NextResponse("not found", { status: res.status });
  }
  const text = await res.text();
  return new NextResponse(text, {
    headers: {
      "Content-Type": "text/plain; charset=utf-8",
      "Cache-Control": "private, max-age=300",
    },
  });
}
