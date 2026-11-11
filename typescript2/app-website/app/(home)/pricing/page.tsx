// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import { Check } from 'lucide-react';
import { section } from '@/components/home/analytics';
import { BOOK } from '@/components/home/links';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button } from '@/components/home/ui';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';

// The pricing page, in the homepage's BAML lane. The nav links it. The plans, rates and what's
// metered are Boundary Cloud's code (BCS, origin/main: config/bcs.toml, control-plane/README.md,
// control-plane/src/projector/billing.rs). Retention, the BYOC add-on and billing a
// repeated value once are the founders' decisions, not yet in that code. Change this page when
// either changes.

type Plan = {
  data?: [included: string, after: string];
  features: string[];
  lit?: boolean;
  name: string;
  price: string;
  unit: string;
};

const PLANS: Plan[] = [
  {
    features: ['The language and toolchain', 'Runs on your machine'],
    name: 'BAML',
    price: 'Free',
    unit: 'open source',
  },
  {
    data: ['1 GB of data a month', '$4 per GB after'],
    features: ['30-day retention', 'Unlimited spans', 'Every Cloud feature'],
    name: 'Starter',
    price: '$0',
    unit: 'per month',
  },
  {
    data: ['5 GB of data a month', '$3 per GB after'],
    // Same rows in the same order as Starter's, so the two read across; the add-on comes last.
    features: [
      'Custom retention, $0.10 per GB per 30 days',
      'Unlimited spans',
      'Every Cloud feature',
      'Bring your own cloud, as an add-on',
    ],
    lit: true,
    name: 'Pro',
    price: '$250',
    unit: 'per month',
  },
];

// No charge for any of these on any Boundary Cloud plan: only data is metered.
const INCLUDED = [
  'Unlimited spans',
  'Profiles',
  'SQL queries and baml query',
  'Dashboards',
  'Unlimited members',
  'Unlimited projects and environments',
  'API keys',
];

// How the meter works, one fact a line.
const BILLING: [string, string][] = [
  [
    'What counts',
    'The values your programs record: inputs, outputs, logs and network payloads.',
  ],
  ['What doesn’t', 'Spans, profiles, queries and seats.'],
  ['Repeats', 'A value your programs record again is billed once.'],
  ['Retries', 'An upload that’s retried is counted once.'],
  [
    'Retention',
    '30 days included. Longer, on Pro, is $0.10 per GB for each extra 30 days, rounded to the nearest 30.',
  ],
  ['Going over', 'Never blocks ingest or queries.'],
  ['Upgrading', 'Takes effect right away, prorated to your billing date.'],
  ['Canceling', 'Ends at the close of your billing month.'],
];

function Tick({ children }: { children: string }) {
  return (
    <li className="flex items-start gap-2">
      <Check
        aria-hidden="true"
        className="mt-[3px] size-4 shrink-0 text-clover"
        strokeWidth={2.25}
      />
      {children}
    </li>
  );
}

/** The pricing page: BAML is free, and Boundary Cloud has Starter and Pro. */
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
          {PLANS.map((p) => (
            <div
              className={`flex flex-col rounded-xl border p-6 ${p.lit ? 'border-clover/60 bg-clover/[0.07]' : 'border-rule bg-raised'}`}
              key={p.name}
            >
              <p className="flex items-center justify-between gap-3 font-display text-[20px] font-medium text-head">
                {p.name}
                {p.lit && (
                  <span className="rounded-full bg-clover px-2.5 py-0.5 text-[12px] font-medium text-black">
                    Recommended
                  </span>
                )}
              </p>
              <p className="mt-4 font-display text-[40px] leading-none font-medium text-head tabular-nums">
                {p.price}
              </p>
              <p className="mt-1 text-[14px] text-muted">{p.unit}</p>
              <ul className="mt-6 space-y-2 text-[15px] text-ink-2">
                {p.data?.map((d) => (
                  <Tick key={d}>{d}</Tick>
                ))}
                {p.features.map((f) => (
                  <Tick key={f}>{f}</Tick>
                ))}
              </ul>
              <div className="mt-auto pt-8">
                <Button href={BOOK} variant={p.lit ? 'primary' : 'secondary'}>
                  Get early access
                </Button>
              </div>
            </div>
          ))}
        </div>

        <div className="mt-20 md:mt-28 grid gap-12 lg:grid-cols-2">
          <div>
            <h2 className="v4-h3 text-head">Included in every Cloud plan</h2>
            <ul className="mt-6 grid gap-x-6 gap-y-3 text-[16px] text-ink-2 sm:grid-cols-2">
              {INCLUDED.map((f) => (
                <Tick key={f}>{f}</Tick>
              ))}
            </ul>
          </div>
          <div>
            <h2 className="v4-h3 text-head">How billing works</h2>
            <dl className="mt-6 divide-y divide-rule border-y border-rule">
              {BILLING.map(([term, fact]) => (
                <div
                  className="grid gap-1 py-3 sm:grid-cols-[9rem_minmax(0,1fr)] sm:gap-4"
                  key={term}
                >
                  <dt className="text-[15px] text-head">{term}</dt>
                  <dd className="text-[15px] text-ink-2">{fact}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>
      </section>
    </PageShell>
  );
}
