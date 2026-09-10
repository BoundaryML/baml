import { revalidatePath } from "next/cache";
import { NextRequest, NextResponse } from "next/server";
import { currentUser, siteOrigin } from "@/lib/auth";
import { runnerRequest } from "@/lib/runner";

export async function POST(request: NextRequest, { params }: { params: Promise<{ id: string }> }) {
  if (!await currentUser()) return NextResponse.json({ error: "Sign in first." }, { status: 401 });
  if (request.headers.get("origin") !== siteOrigin()) return NextResponse.json({ error: "Invalid origin." }, { status: 403 });
  const { id } = await params;
  if (!/^ISSUE-[A-Za-z0-9_-]{1,120}$/.test(id)) return NextResponse.json({ error: "Invalid issue." }, { status: 400 });
  try {
    if (Number(request.headers.get("content-length") ?? 0) > 24_000) throw new Error("Request too large.");
    const reader = request.body?.getReader();
    if (!reader) throw new Error("Request body is missing.");
    const chunks: Uint8Array[] = []; let size = 0;
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 24_000) { await reader.cancel(); throw new Error("Request too large."); }
      chunks.push(value);
    }
    const input = JSON.parse(Buffer.concat(chunks).toString("utf8"));
    if (!["comment", "export"].includes(input.operation) || !["live", "eval"].includes(input.dataset)) throw new Error("Invalid request.");
    if (input.operation === "comment" && (typeof input.body !== "string" || !input.body.trim() || input.body.length > 5000)) throw new Error("Comment must contain 1–5000 characters.");
    const result = await runnerRequest(input.operation, { id, dataset: input.dataset, body: input.body });
    revalidatePath(`/issues/${id}`);
    return NextResponse.json(result, { headers: { "Cache-Control": "no-store" } });
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : "Request failed." }, { status: 400 });
  }
}
