import { loadState } from '../../lib/data';

// Polled by the live dashboard every few seconds. Runs server-side on the
// UI machine, which reaches the api over .internal (token stays server-side).
export const dynamic = 'force-dynamic';

type LiveStateT = Awaited<ReturnType<typeof loadState>>;

// Short-TTL single-flight cache. Each /api/state call used to fan out to four
// heavy api queries (/tasks?limit=200, /trophies, /issues, /bamlBuilds) with no
// caching, so api load scaled with (open tabs x ui machines) and could saturate
// the api. This caches the snapshot for STATE_TTL_MS and collapses concurrent
// requests onto one in-flight fetch, so the api is hit at most ~once per TTL per
// ui machine regardless of how many clients poll.
const STATE_TTL_MS = Number(process.env.STATE_TTL_MS ?? 2500);
let cached: { at: number; data: LiveStateT } | null = null;
let inflight: Promise<LiveStateT> | null = null;

async function getState(): Promise<LiveStateT> {
  const now = Date.now();
  if (cached && now - cached.at < STATE_TTL_MS) return cached.data;
  if (inflight) return inflight; // collapse concurrent refreshes onto one fetch
  inflight = loadState()
    .then((data) => {
      cached = { at: Date.now(), data };
      return data;
    })
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/**
 * GET /api/state - returns the current live snapshot as JSON for the polling dashboard.
 * @returns a JSON Response wrapping the LiveState from loadState() (cached up to STATE_TTL_MS)
 */
export async function GET() {
  return Response.json(await getState());
}
