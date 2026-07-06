import { createMetadata } from '@/app/_lib/metadata';
import { BenchmarkingClient } from '@/components/landing/benchmarking-client';
import { ForceLightTheme } from '@/components/force-light-theme';
import { Navbar } from '@/components/navbar';

export const metadata = createMetadata({
  description:
    'How we benchmark BAML: agents and humans as instrumented users of the language, plus an adherence score that grades codebases against our recorded design intent.',
  eyebrow: 'Benchmarking',
  path: '/benchmarking',
  title:
    'How we do benchmarking: testing BAML by testing agents on how well they can test BAML',
});

export default function BenchmarkingPage() {
  return (
    <div className="w-full bg-background text-foreground">
      <ForceLightTheme />
      <Navbar />
      <BenchmarkingClient />
    </div>
  );
}
