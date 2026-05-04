import type { Metadata } from 'next';
import { ForceLightTheme } from '@/components/force-light-theme';
import { ThesisClient } from '@/components/landing/thesis-client';
import { Navbar } from '@/components/navbar';

export const metadata: Metadata = {
  description:
    'The BAML thesis: open-source schemas, code generation, and tooling for ML APIs.',
  title: 'The BAML Thesis',
};

export default function ThesisPage() {
  return (
    <div className="w-full bg-background text-foreground">
      <ForceLightTheme />
      <Navbar />
      <ThesisClient />
    </div>
  );
}
