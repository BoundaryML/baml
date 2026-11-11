// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import { Check } from 'lucide-react';
import type { ReactNode } from 'react';
import { section } from '@/components/home/analytics';
import { BOOK } from '@/components/home/links';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';

// The pricing page, in the homepage's BAML lane. The nav links it. Laid out after Better Stack's
// pricing page: a centered head with the guarantee, a wide card for the free tier, then the paid
// plans and one line-item table, each price set as an amount with its unit small under it.
//
// The plans, rates and what's metered are Boundary Cloud's code (BCS, origin/main: config/bcs.toml,
// control-plane/README.md, control-plane/src/projector/billing.rs). Retention pricing, the BYOC
// add-on and the money-back guarantee are the founders' decisions, not yet in that code. Change this
// page when either changes.

/** A table cell: an amount and, for a price, its unit under it. */
type Cell = [amount: string, unit?: string];

// The line items in groups, the way Better Stack splits its table by product. Each row is Starter's
// cell then Pro's. Every non-price fact is from BCS: spans, profiles and seats are never billed,
// there's no usage cap (billing never blocks ingest or queries), roles are admin and member, keys can be scoped to an environment.
const SAME = (cell: Cell): [Cell, Cell] => [cell, cell];

const GROUPS: {
  rows: { cells: [Cell, Cell]; label: string }[];
  title: string;
}[] = [
  {
    rows: [
      {
        cells: [
          ['1 GB', 'per month'],
          ['5 GB', 'per month'],
        ],
        label: 'Data included',
      },
      {
        cells: [
          ['$4', 'per GB'],
          ['$3', 'per GB'],
        ],
        label: 'Data after that',
      },
      {
        cells: SAME(['None', 'extra data is billed']),
        label: 'Usage cap',
      },
      { cells: SAME(['Unlimited']), label: 'Spans' },
      { cells: SAME(['Unlimited']), label: 'Profiles' },
    ],
    title: 'Data',
  },
  {
    rows: [
      { cells: SAME(['30 days']), label: 'Included' },
      {
        cells: [['—'], ['$0.10', 'per GB per 30 days']],
        label: 'Longer retention',
      },
    ],
    title: 'Retention',
  },
  {
    rows: [
      { cells: SAME(['Included']), label: 'SQL queries and baml query' },
      { cells: SAME(['Unlimited']), label: 'Dashboards' },
    ],
    title: 'Capabilities',
  },
  {
    rows: [
      { cells: SAME(['Unlimited']), label: 'Members' },
      { cells: SAME(['Unlimited']), label: 'Projects and environments' },
      { cells: SAME(['Admin and member']), label: 'Roles' },
    ],
    title: 'Team',
  },
  {
    rows: [
      { cells: SAME(['Unlimited']), label: 'API keys, scoped by environment' },
      { cells: [['—'], ['Add-on']], label: 'Bring your own cloud' },
    ],
    title: 'Governance',
  },
];

const PLANS = [
  { name: 'Starter', price: '$0' },
  { lit: true, name: 'Pro', price: '$250' },
];

// Label, Starter, Pro.
const COLUMNS =
  'grid grid-cols-[minmax(0,1fr)_6.5rem_8.5rem] gap-4 px-5 sm:grid-cols-[minmax(0,1fr)_9rem_11rem]';

function Tick({ children }: { children: ReactNode }) {
  return (
    <li className="flex items-start gap-2.5">
      <Check
        aria-hidden="true"
        className="mt-[3px] size-4 shrink-0 text-clover"
        strokeWidth={2.25}
      />
      {children}
    </li>
  );
}

function Amount({ cell }: { cell: Cell }) {
  return (
    <dd className="text-right">
      <span className="block text-head">{cell[0]}</span>
      {cell[1] && (
        <span className="block text-[13px] text-muted">{cell[1]}</span>
      )}
    </dd>
  );
}

/** The pricing page: BAML is free, and Boundary Cloud bills data and retention, on Starter or Pro. */
export default function PricingPage() {
  return (
    <PageShell>
      <SoftwareApplicationStructuredData />
      <section
        {...section('pricing_header')}
        className={`${PAGE_WRAP} pt-16 md:pt-24 pb-20 md:pb-32`}
      >
        <div className="text-center">
          <h1 className="v4-h1">Pricing</h1>
          <p className="mt-5 v4-body text-ink-2">
            Every Boundary Cloud plan comes with a 60-day money-back guarantee.
          </p>
        </div>

        <div className="mx-auto mt-14 max-w-[960px] space-y-20 md:mt-20">
          {/* The free tier, wide, the way Better Stack opens with theirs. */}
          <div className="rounded-xl border border-rule bg-raised p-6 md:p-8">
            <div className="flex flex-wrap items-end justify-between gap-6">
              <div>
                <p className="text-[15px] font-medium text-head">
                  BAML, free and open source
                </p>
                <p className="mt-2 font-display text-[40px] leading-none font-medium text-head">
                  $0
                  <span className="ml-2 font-sans text-[15px] font-normal text-muted">
                    forever
                  </span>
                </p>
              </div>
              <Button href={LINKS.quickstart} variant="secondary">
                Install BAML
              </Button>
            </div>
            <ul className="mt-6 grid gap-x-8 gap-y-2.5 border-t border-rule pt-6 text-[15px] text-ink-2 sm:grid-cols-2">
              <Tick>The language, compiler and toolchain</Tick>
              <Tick>Runs on your machine</Tick>
              <Tick>Never calls our servers</Tick>
              <Tick>Source on GitHub</Tick>
            </ul>
          </div>

          <div>
            <div className="border-b border-rule pb-5">
              <h2 className="v4-h3 text-head">Boundary Cloud</h2>
              <p className="mt-2 text-ink-2">
                You pay for data and retention. Spans, seats, queries and
                profiles are free.
              </p>
            </div>

            <div className="mt-8 grid gap-4 sm:grid-cols-2">
              {PLANS.map((p) => (
                <div
                  className={`flex flex-wrap items-end justify-between gap-6 rounded-xl border p-6 ${p.lit ? 'border-clover/60 bg-clover/[0.07]' : 'border-rule bg-raised'}`}
                  key={p.name}
                >
                  <div>
                    <p className="text-[15px] font-medium text-head">
                      {p.name}
                    </p>
                    <p className="mt-2 font-display text-[40px] leading-none font-medium text-head tabular-nums">
                      {p.price}
                    </p>
                    <p className="mt-1 text-[13px] text-muted">per month</p>
                  </div>
                  <Button href={BOOK} variant={p.lit ? 'primary' : 'secondary'}>
                    Get early access
                  </Button>
                </div>
              ))}
            </div>

            {/* One line-item table for both plans: the item, then Starter, then Pro. */}
            <div className="mt-4 overflow-hidden rounded-xl border border-rule">
              <div
                className={`${COLUMNS} border-b border-rule bg-raised py-3 text-[13px] text-muted`}
              >
                <span />
                <span className="text-right">Starter</span>
                <span className="text-right">Pro</span>
              </div>
              {GROUPS.map((g) => (
                <div className="[&:last-child_dl]:border-b-0" key={g.title}>
                  <p
                    className={`${COLUMNS} border-b border-rule bg-white/[0.02] py-2.5 font-mono text-[12px] uppercase tracking-[0.08em] text-muted`}
                  >
                    {g.title}
                  </p>
                  <dl className="divide-y divide-rule border-b border-rule text-[15px]">
                    {g.rows.map((r) => (
                      <div
                        className={`${COLUMNS} items-baseline py-3`}
                        key={r.label}
                      >
                        <dt className="text-ink-2">{r.label}</dt>
                        <Amount cell={r.cells[0]} />
                        <Amount cell={r.cells[1]} />
                      </div>
                    ))}
                  </dl>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>
    </PageShell>
  );
}
