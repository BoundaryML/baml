import { SlackLink } from "@/components/slack-link";

export default function RunsPage() {
  return <main className="max-w-3xl mx-auto p-8 space-y-4">
    <h1 className="text-2xl font-semibold">Bammy runs</h1>
    <p>Read task summaries and ask follow-up questions in the original Slack thread. Prompts and transcripts are private and are not published on this website.</p>
    <SlackLink>Open Slack</SlackLink>
  </main>;
}
