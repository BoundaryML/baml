import { notFound } from "next/navigation";
import Link from "next/link";
import { LiveAgents } from "@/components/live-agents";
export default async function AgentPage({ params, searchParams }: { params: Promise<{ id: string }>; searchParams: Promise<{ dataset?: string }> }) {
  const { id } = await params;
  const dataset = (await searchParams).dataset === "eval" ? "eval" : "live";
  if (!/^(?:[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}|[0-9a-f]{24}-(?:reproduce|investigate|ticket|deduplicate|fix))$/.test(id)) notFound();
  return <main className="mx-auto max-w-5xl space-y-6 p-6"><Link href={`/agents?dataset=${dataset}`} className="text-xs text-muted-foreground hover:text-foreground">← Agent sessions</Link><LiveAgents id={id} dataset={dataset} /></main>;
}
