import { notFound } from "next/navigation";
import Link from "next/link";
import { LiveAgents } from "@/components/live-agents";
export default async function AgentPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  if (!/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/.test(id)) notFound();
  return <main className="mx-auto max-w-5xl space-y-6 p-6"><Link href="/agents" className="underline">All agent sessions</Link><LiveAgents id={id} /></main>;
}
