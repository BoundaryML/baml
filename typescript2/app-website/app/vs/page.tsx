import { createMetadata } from '@/app/_lib/metadata';
import { ForceLightTheme } from '@/components/force-light-theme';
import { VsClient } from '@/components/landing/vs-client';
import { Navbar } from '@/components/navbar';

export const metadata = createMetadata({
  description:
    'How BAML compares to Python, TypeScript, Go, Rust, LangGraph, and the AI SDK for typed LLM code.',
  ogTitle: 'BAML vs the rest',
  path: '/vs',
  title: 'BAML vs X',
});

export default function VsPage() {
  return (
    <div className="w-full bg-background text-foreground">
      <ForceLightTheme />
      <Navbar />
      <VsClient />
    </div>
  );
}
