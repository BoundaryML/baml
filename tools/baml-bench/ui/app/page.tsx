import GraphView from "./graph-view";
import { loadState } from "./lib/data";

export const dynamic = "force-dynamic";

/**
 * Server component for the dashboard home route ("/"). Loads the initial live
 * state on the server and hands it to the client GraphView for live polling.
 * @returns the GraphView seeded with the initial LiveState
 */
export default async function Page() {
  const initial = await loadState();
  return <GraphView initial={initial} />;
}
