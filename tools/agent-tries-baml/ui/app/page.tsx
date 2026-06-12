import AgentsView from './agents-view';
import { loadState } from './lib/data';

export const dynamic = 'force-dynamic';

/**
 * Server component for the home route ("/") — the agents.boundaryml.com view:
 * a live roster of every agent in the monolith plus dispatched work. Loads
 * the initial live state on the server and hands it to the client AgentsView
 * for live polling. The original pipeline dashboard lives at /pipeline.
 * @returns the AgentsView seeded with the initial LiveState
 */
export default async function Page() {
  const initial = await loadState();
  return <AgentsView initial={initial} />;
}
