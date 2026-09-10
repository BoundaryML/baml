import { currentUser } from "@/lib/auth";
import { runnerRequest } from "@/lib/runner";
export const dynamic = "force-dynamic";
export async function GET(request: Request) {
  if (!await currentUser()) return Response.json({ error: "Sign in with GitHub to view transcripts." }, { status: 401 });
  const query = new URL(request.url).searchParams;
  const id = query.get("id") ?? "";
  const offset = Number(query.get("offset") ?? "0");
  const issue = query.get("issue_id") ?? "";
  const pr = query.get("pr") ?? "";
  if (pr && !/^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/.test(pr)) return Response.json({ error: "Invalid PR" }, { status: 400 });
  const dataset = query.get("dataset") ?? "live";
  if ((id && !/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/.test(id)) || !Number.isSafeInteger(offset) || offset < 0 || (issue && !/^ISSUE-[A-Za-z0-9_-]+$/.test(issue)) || !["live", "eval"].includes(dataset)) return Response.json({ error: "Invalid transcript request" }, { status: 400 });
  try { return Response.json(await runnerRequest("traces", { id, offset, issue_id: issue, dataset, pr }), { headers: { "Cache-Control": "private, no-store" } }); }
  catch { return Response.json({ error: "Live transcripts are unavailable. The runner may need updating." }, { status: 503 }); }
}
