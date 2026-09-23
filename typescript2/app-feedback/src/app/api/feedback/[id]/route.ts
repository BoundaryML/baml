import { NextResponse } from "next/server";
import { loadFeedback } from "@/lib/db";

export async function GET(_request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  try {
    const report = await loadFeedback(id);
    return NextResponse.json(report ?? { error: "Report not found." }, { status: report ? 200 : 404 });
  } catch {
    return NextResponse.json({ error: "Could not load this report. Please try again." }, { status: 503 });
  }
}
