import { IntuitionCard } from "@/components/intuition-card";
import { LiveUpdates } from "@/components/issues/live-updates";
import { dataSource, loadIntuitions } from "@/lib/db";

export const revalidate = 0;

export default async function IntuitionPage() {
  const intuitions = await loadIntuitions();
  return <main className="mx-auto max-w-4xl space-y-6 px-4 py-6">
    <LiveUpdates />
    <div>
      <h1 className="text-2xl font-semibold">Intuition</h1>
      <p className="mt-1 text-sm text-muted-foreground">
        What the issues say together that no single ticket says: one cause under several tickets, a subsystem that keeps
        breaking, a class of report the pipeline keeps turning away. Rewritten by an Opus pass whenever an issue changes;
        every finding cites the issues that support it.
      </p>
    </div>
    {intuitions.length === 0
      ? <p className="text-sm text-muted-foreground">
          {dataSource === "supabase" ? "No intuitions yet: the pass runs after the next pipeline run with at least two issues." : "Mock data: intuitions need the store."}
        </p>
      : <div className="space-y-4">{intuitions.map((i) => <IntuitionCard key={i.id} intuition={i} />)}</div>}
  </main>;
}
