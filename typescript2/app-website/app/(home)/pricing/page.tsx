// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';

// The pricing page, in the homepage's BAML lane. It isn't linked from the nav or the footer.

export default function PricingPage() {
  return (
    <PageShell>
      <SoftwareApplicationStructuredData />
      <section
        {...section('pricing_header')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-20 md:pb-32`}
      >
        <h1 className="v4-h1 max-w-[12em]">BAML is free and open source.</h1>
        <p className="mt-6 md:mt-8 max-w-[36rem] v4-body text-ink-2">
          It runs entirely on your machine. It never calls our servers.
        </p>
        <div className="mt-8">
          <Button href={LINKS.quickstart}>Get started</Button>
        </div>
        <p className="mt-12 md:mt-16 pt-6 max-w-[36rem] border-t border-rule text-[16px] leading-[1.5] text-body">
          Cloud is coming later this year. Observability, team controls, and
          governance.
        </p>
      </section>
    </PageShell>
  );
}
