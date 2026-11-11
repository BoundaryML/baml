// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import type { ReactNode } from 'react';
import { section } from '@/components/home/analytics';
import { BOOK } from '@/components/home/links';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';

// The pricing page, in the homepage's BAML lane. The nav links it. The plans, rates and what's
// metered are Boundary Cloud's code (BCS, origin/main: config/bcs.toml, control-plane/README.md,
// control-plane/src/projector/billing.rs). Retention pricing, the BYOC add-on and the money-back
// guarantee are the founders'
// decisions, not yet in that code. Change this page when either changes.

// The rows the two Cloud plans differ on, in the same order on both cards so they read across.
const ROWS = [
  'Data included',
  'After that',
  'Retention',
  'Longer retention',
  'Your own cloud',
];

const CLOUD: { cells: string[]; lit?: boolean; name: string; price: string }[] =
  [
    {
      cells: ['1 GB a month', '$4 per GB', '30 days', '—', '—'],
      name: 'Starter',
      price: '$0',
    },
    {
      cells: [
        '5 GB a month',
        '$3 per GB',
        '30 days',
        '$0.10 per GB per 30 days',
        'Add-on',
      ],
      lit: true,
      name: 'Pro',
      price: '$250',
    },
  ];

// Never billed, on either plan: only data and retention are.
const FREE = [
  'Spans',
  'Profiles',
  'Queries',
  'Dashboards',
  'Members',
  'Projects',
  'API keys',
];

function Card({
  children,
  lit,
  name,
  price,
  unit,
}: {
  children: ReactNode;
  lit?: boolean;
  name: string;
  price: string;
  unit: string;
}) {
  return (
    <div
      className={`flex flex-col rounded-xl border p-6 ${lit ? 'border-clover/60 bg-clover/[0.07]' : 'border-rule bg-raised'}`}
    >
      <p className="font-display text-[20px] font-medium text-head">{name}</p>
      <p className="mt-4 font-display text-[40px] leading-none font-medium text-head tabular-nums">
        {price}
      </p>
      <p className="mt-1 text-[14px] text-muted">{unit}</p>
      {children}
    </div>
  );
}

/** The pricing page: BAML is free, and Boundary Cloud bills data and retention, on Starter or Pro. */
export default function PricingPage() {
  return (
    <PageShell>
      <SoftwareApplicationStructuredData />
      <section
        {...section('pricing_header')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-20 md:pb-32`}
      >
        <h1 className="v4-h1 max-w-[12em]">Pricing</h1>
        <p className="mt-6 max-w-[36rem] v4-body text-ink-2">
          You pay for data and retention. Spans, seats, queries and profiles are
          free.
        </p>

        <div className="mt-12 md:mt-16 grid gap-4 md:grid-cols-3">
          <Card name="BAML" price="Free" unit="open source">
            <p className="mt-6 text-[15px] text-ink-2">
              The language and toolchain. Runs on your machine.
            </p>
            <div className="mt-auto pt-8">
              <Button href={LINKS.quickstart} variant="secondary">
                Install BAML
              </Button>
            </div>
          </Card>

          {CLOUD.map((p) => (
            <Card
              key={p.name}
              lit={p.lit}
              name={p.name}
              price={p.price}
              unit="per month"
            >
              <dl className="mt-6 divide-y divide-rule border-y border-rule text-[15px]">
                {ROWS.map((row, i) => (
                  <div className="flex justify-between gap-4 py-2.5" key={row}>
                    <dt className="shrink-0 text-muted">{row}</dt>
                    <dd className="text-right text-head">{p.cells[i]}</dd>
                  </div>
                ))}
              </dl>
              <div className="mt-auto pt-8">
                <Button href={BOOK} variant={p.lit ? 'primary' : 'secondary'}>
                  Get early access
                </Button>
              </div>
            </Card>
          ))}
        </div>

        <div className="mt-16 md:mt-20 max-w-[44rem]">
          <h2 className="v4-h3 text-head">Free on both plans</h2>
          <p className="mt-3 text-[17px] text-ink-2">{FREE.join(' · ')}</p>
          <ul className="mt-8 space-y-1.5 text-[15px] text-muted">
            <li>60-day money-back guarantee.</li>
            <li>
              Going over your included data never blocks ingest or queries.
            </li>
          </ul>
        </div>
      </section>
    </PageShell>
  );
}
