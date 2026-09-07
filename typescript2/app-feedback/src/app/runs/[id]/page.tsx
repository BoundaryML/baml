import { notFound } from "next/navigation";
import RunsPage from "../page";

export default async function RunPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  if (!/^[1-9][0-9]{0,18}$/.test(id)) notFound();
  return <RunsPage />;
}
