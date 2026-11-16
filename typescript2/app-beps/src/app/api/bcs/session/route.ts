import { NextResponse } from "next/server";
import { currentViewer } from "@/lib/bcs-auth";

export const dynamic = "force-dynamic";

/** Whether the viewer may read BCS proposals, so the UI can show the link to them. */
export async function GET() {
  const viewer = await currentViewer();
  return NextResponse.json(
    { member: viewer !== null, login: viewer?.login ?? null },
    { headers: { "Cache-Control": "no-store" } }
  );
}
