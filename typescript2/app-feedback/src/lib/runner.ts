import { createHmac } from "node:crypto";
import { currentUser } from "./auth";

export async function runnerRequest<T>(operation: string, input: Record<string, unknown> = {}): Promise<T> {
  const author = await currentUser();
  if (!author) throw new Error("Sign in with GitHub first.");
  const secret = process.env.ATB2_UI_RUNNER_SECRET;
  const url = new URL(process.env.ATB2_RUNNER_URL ?? "https://atb2-runner.fly.dev");
  if (!secret || secret.length < 32 || url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) throw new Error("Runner access is not configured.");
  const body = JSON.stringify({ ...input, operation, author });
  const timestamp = Math.floor(Date.now() / 1000).toString();
  const signature = "v0=" + createHmac("sha256", secret).update(`v0:${timestamp}:${body}`).digest("hex");
  const response = await fetch(new URL("/ui", url), { method: "POST", body,
    headers: { "Content-Type": "application/json", "X-ATB2-Timestamp": timestamp, "X-ATB2-Signature": signature },
    cache: "no-store", redirect: "error", signal: AbortSignal.timeout(operation === "export" ? 120_000 : 30_000) });
  if (!response.ok) throw new Error(response.status === 409 ? "The issue changed. Refresh and try again." : "The runner could not complete this request.");
  return response.json() as Promise<T>;
}
