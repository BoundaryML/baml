import { notFound } from "next/navigation";
import { Activity } from "@/components/issues/activity";
import { LiveUpdates } from "@/components/issues/live-updates";
import { loadPrEvents } from "@/lib/db";

export const dynamic = "force-dynamic";
export default async function PrPage({ params, searchParams }: {
  params: Promise<{ number: string }>; searchParams: Promise<{ dataset?: string }>;
}) {
  const { number } = await params;
  if (!/^[1-9][0-9]{0,9}$/.test(number)) notFound();
  const dataset = (await searchParams).dataset === "eval" ? "eval" : "live";
  const events = await loadPrEvents(number, dataset);
  return <main className="max-w-4xl mx-auto p-8 space-y-6"><LiveUpdates />
    <div><h1 className="text-2xl font-semibold">Babysitter · PR #{number}</h1>
      <a className="underline" href={`https://github.com/BoundaryML/baml/pull/${number}`}>View PR on GitHub</a>
      <p className="text-sm text-muted-foreground">{dataset} · Updates every 30 seconds. Each proposed fix requires approval. Review proposed fixes here and approve them on Slack.</p>
    </div><Activity events={events} dataset={dataset} />
  </main>;
}
