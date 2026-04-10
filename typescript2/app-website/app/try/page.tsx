'use client';

import dynamic from 'next/dynamic';
import { Navbar } from '@/components/navbar';
import { FooterSection } from '@/components/footer-section';
import { Sparkles } from 'lucide-react';

const BamlPlayground = dynamic(
  () => import('@/playground/BamlPlayground').then((m) => m.BamlPlayground),
  {
    ssr: false,
    loading: () => (
      <div className="flex items-center justify-center min-h-[400px] text-muted-foreground">
        Loading playground...
      </div>
    ),
  },
);

export default function TryPage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center divide-y divide-border min-h-screen w-full">
        {/* Header */}
        <section className="w-full px-6 py-10 text-center">
          <div className="flex items-center gap-2 rounded-full bg-primary/10 px-4 py-2 text-sm font-medium mx-auto w-fit mb-4">
            <Sparkles className="h-4 w-4" />
            Interactive Playground
          </div>
          <h1 className="text-3xl font-bold tracking-tight sm:text-4xl max-w-3xl mx-auto">
            Try BAML in Your Browser
          </h1>
          <p className="mt-3 text-muted-foreground max-w-xl mx-auto">
            Edit BAML code, run functions against real LLMs, and see results instantly.
            Powered by the BAML compiler running in WebAssembly.
          </p>
        </section>

        {/* Playground */}
        <section className="w-full px-4 py-6">
          <BamlPlayground />
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
