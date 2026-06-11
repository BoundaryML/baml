import { createMetadata } from '@/app/_lib/metadata';
import { ForceLightTheme } from '@/components/force-light-theme';
import { Navbar } from '@/components/navbar';
import { VsClient } from '@/components/landing/vs-client';

export const metadata = createMetadata({
  description:
    'How BAML compares to Python, TypeScript, Go, and Rust for building typed LLM apps and agents.',
  eyebrow: 'Comparison',
  ogTitle: 'BAML vs Python, TypeScript, Go & Rust',
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
