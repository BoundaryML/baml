import type { Metadata } from 'next';
import { ForceLightTheme } from '@/components/force-light-theme';
import { Navbar } from '@/components/navbar';
import { VsClient } from '@/components/landing/vs-client';

export const metadata: Metadata = {
  description:
    'How BAML compares to Python, TypeScript, Go, and Rust for building typed LLM apps and agents.',
  title: 'BAML vs X',
};

export default function VsPage() {
  return (
    <div className="w-full bg-background text-foreground">
      <ForceLightTheme />
      <Navbar />
      <VsClient />
    </div>
  );
}
