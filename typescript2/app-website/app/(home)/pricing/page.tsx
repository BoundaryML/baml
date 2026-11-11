// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import { section } from '@/components/home/analytics';
import { BOOK } from '@/components/home/links';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';

// The pricing page, in the homepage's BAML lane. The nav links it. The plans and their numbers are
// Boundary Cloud's billing catalog (BCS: config/bcs.toml and control-plane/README.md); change them
// here when that catalog changes.

const PLANS: {
  data: string;
  included: string;
  name: string;
  price: string;
}[] = [
  {
    data: '$4 per GB after that',
    included: '1 GB of data a month',
    name: 'Starter',
    price: '$0',
  },
  {
    data: '$3 per GB after that',
    included: '5 GB of data a month',
    name: 'Pro',
    price: '$250',
  },
];

/** The pricing page: BAML itself is free, and Boundary Cloud has two plans. */
export default function PricingPage() {
  return (
    <PageShell>
      <SoftwareApplicationStructuredData />
      <section
        {...section('pricing_header')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-20 md:pb-32`}
      >
        <h1 className="v4-h1 max-w-[12em]">Pricing</h1>

        <div className="mt-12 md:mt-16 grid gap-4 md:grid-cols-3">
          <div className="flex flex-col rounded-xl border border-rule bg-raised p-6">
            <p className="font-display text-[20px] font-medium text-head">
              BAML
            </p>
            <p className="mt-4 font-display text-[40px] leading-none font-medium text-head">
              Free
            </p>
            <p className="mt-1 text-[14px] text-muted">open source</p>
            <ul className="mt-6 space-y-2 text-[15px] text-ink-2">
              <li>The language and the whole toolchain</li>
              <li>Runs on your machine, never calls our servers</li>
            </ul>
            <div className="mt-auto pt-8">
              <Button href={LINKS.quickstart} variant="secondary">
                Install BAML
              </Button>
            </div>
          </div>

          {PLANS.map((p) => (
            <div
              className="flex flex-col rounded-xl border border-rule bg-raised p-6"
              key={p.name}
            >
              <p className="font-display text-[20px] font-medium text-head">
                Boundary Cloud <span className="text-clover">{p.name}</span>
              </p>
              <p className="mt-4 font-display text-[40px] leading-none font-medium text-head tabular-nums">
                {p.price}
              </p>
              <p className="mt-1 text-[14px] text-muted">per month</p>
              <ul className="mt-6 space-y-2 text-[15px] text-ink-2">
                <li>Unlimited spans</li>
                <li>{p.included}</li>
                <li>{p.data}</li>
              </ul>
              <div className="mt-auto pt-8">
                <Button href={BOOK}>Get early access</Button>
              </div>
            </div>
          ))}
        </div>

        <p className="mt-8 max-w-[40rem] text-[15px] leading-[1.5] text-muted">
          Boundary Cloud is in early access. Data is what your programs send to
          it, counted in decimal GB each billing month.
        </p>
      </section>
    </PageShell>
  );
}
