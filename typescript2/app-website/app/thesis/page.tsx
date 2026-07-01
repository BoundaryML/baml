import { createMetadata } from '@/app/_lib/metadata';
import { ForceLightTheme } from '@/components/force-light-theme';
import { ThesisClient } from '@/components/landing/thesis-client';
import { Navbar } from '@/components/navbar';

export const metadata = createMetadata({
  description:
    'The BAML thesis: a programming language that agents are good at writing and humans are good at understanding.',
  eyebrow: 'Thesis',
  path: '/thesis',
  title: 'The BAML Thesis',
});

export default function ThesisPage() {
  return (
    <div className="w-full bg-background text-foreground">
      <ForceLightTheme />
      <Navbar />
      <ThesisClient />
    </div>
  );
}
