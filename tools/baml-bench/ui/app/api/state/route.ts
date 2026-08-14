import { loadState } from "../../lib/data";

// Polled by the live dashboard every few seconds. Runs server-side on the
// UI machine, which reaches the api over .internal (token stays server-side).
export const dynamic = "force-dynamic";

/**
 * GET /api/state - returns the current live snapshot as JSON for the polling dashboard.
 * @returns a JSON Response wrapping the LiveState from loadState()
 */
export async function GET() {
  return Response.json(await loadState());
}
