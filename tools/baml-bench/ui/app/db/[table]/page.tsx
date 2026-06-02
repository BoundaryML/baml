import { notFound } from "next/navigation";

import { loadState } from "../../lib/data";
import DbView from "./db-view";

export const dynamic = "force-dynamic";

const TABLES = ["tasks", "trophies", "issues"];

/**
 * Server component for the "/db/[table]" route. Validates the table slug against
 * the allowed tasks/trophies/issues set (404s otherwise), loads the initial live
 * state, and renders the client DbView.
 * @param params - the route params resolving to the requested table slug
 * @returns the DbView for the table, or a not-found response for unknown tables
 */
export default async function Page({ params }: { params: Promise<{ table: string }> }) {
  const { table } = await params;
  if (!TABLES.includes(table)) notFound();
  const initial = await loadState();
  return <DbView table={table} initial={initial} />;
}
