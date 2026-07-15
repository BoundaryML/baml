import Link from 'next/link';
// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';
import { Button } from '@/components/ui/button';

export default function PricingPage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <SoftwareApplicationStructuredData />
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        <section className="relative flex w-full flex-1 items-center justify-center px-4 py-20 md:py-32">
          <div className="flex flex-col items-center justify-center space-y-8 text-center">
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl md:text-6xl lg:text-7xl max-w-4xl">
              BAML is free and open source.
            </h1>
            <p className="max-w-xl text-lg text-muted-foreground">
              It runs entirely on your machine. It never calls our servers.
            </p>
            <Button asChild size="lg">
              <Link href="/quickstart">Get started</Link>
            </Button>
            <p className="max-w-xl text-sm text-muted-foreground">
              Cloud is coming later this year. Observability, team controls, and
              governance.
            </p>
          </div>
        </section>
        <FooterSection />
      </main>
    </div>
  );
}
