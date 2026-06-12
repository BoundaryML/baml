import GraphView from '../graph-view';
import { loadState } from '../lib/data';

export const dynamic = 'force-dynamic';

/**
 * Server component for the pipeline route ("/pipeline") — the original
 * dashboard home (graph + issue board + runs), moved here when "/" became
 * the agents roster. Loads the initial live state on the server and hands it
 * to the client GraphView for live polling.
 * @returns the GraphView seeded with the initial LiveState
 */
export default async function Page() {
  const initial = await loadState();
  return <GraphView initial={initial} />;
}
