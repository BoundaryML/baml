// CONTENT PARITY: keep substantive copy in sync with content/pricing.md.
import { Check, Plus } from 'lucide-react';
import type { ReactNode } from 'react';
import { section } from '@/components/home/analytics';
import { BOOK } from '@/components/home/links';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { SoftwareApplicationStructuredData } from '@/components/structured-data';

// The pricing page, in the homepage's BAML lane. The nav links it. Laid out after Better Stack's
// pricing page: a centered head with the guarantee, a wide card for the free tier, the paid plans,
// then one block per topic, each a heading, a line, a compact table (a bold row for what's
// included, the paid extras under it on a plus) and a checklist of what's free.
//
// Every claim is one of: Boundary Cloud's code (BCS, origin/main: config/bcs.toml,
// control-plane/README.md, control-plane/src/projector/billing.rs), or a founders' decision
// (retention pricing, the BYOC add-on, the money-back guarantee, what's coming soon). Nothing here
// is a feature BCS doesn't have unless it says "Coming soon".

/** A cell: an amount and, for a price, its unit under it. */
type Cell = [amount: string, unit?: string];

/** A row: Starter's cell, then Pro's. `extra` rows are the paid or optional ones, set under the
 *  bold row they extend. */
type Row = { cells: [Cell, Cell]; extra?: boolean; label: string };

const SAME = (cell: Cell): [Cell, Cell] => [cell, cell];

const BLOCKS: { free: string[]; note: string; rows: Row[]; title: string }[] = [
  {
    free: [
      'Unlimited spans',
      'Unlimited profiles',
      'SQL queries and baml query',
      'Unlimited dashboards',
    ],
    note: 'What your programs record: every function’s inputs, outputs, logs and network calls.',
    // Named the way Better Stack names its line items: what's included, then ingestion and
    // retention past it.
    rows: [
      {
        cells: [
          ['1 GB', 'a month, kept 30 days'],
          ['5 GB', 'a month, kept 30 days'],
        ],
        label: 'Included',
      },
      {
        cells: [
          ['$4', 'per GB'],
          ['$3', 'per GB'],
        ],
        extra: true,
        label: 'Ingestion',
      },
      {
        cells: [['—'], ['$0.10', 'per GB per 30 days']],
        extra: true,
        label: 'Retention',
      },
    ],
    title: 'Data',
  },
  {
    free: ['Unlimited members', 'Unlimited projects and environments'],
    note: 'Sign in with Google today; single sign-on for your identity provider is coming.',
    rows: [
      { cells: SAME(['Included']), label: 'Sign in with Google' },
      {
        cells: [['—'], ['Coming soon']],
        extra: true,
        label: 'SAML and OIDC single sign-on',
      },
      {
        cells: [['—'], ['Coming soon']],
        extra: true,
        label: 'SCIM directory sync',
      },
      {
        cells: SAME(['Coming soon']),
        extra: true,
        label: 'Audit logs',
      },
    ],
    title: 'Team and security',
  },
  {
    free: [],
    note: 'We host it, or it runs in your own cloud account and your data stays there.',
    rows: [
      { cells: SAME(['Included']), label: 'Hosted by Boundary' },
      {
        cells: [['—'], ['Add-on']],
        extra: true,
        label: 'Bring your own cloud',
      },
      {
        cells: SAME(['Coming soon']),
        extra: true,
        label: 'Usage caps',
      },
    ],
    title: 'Deployment',
  },
];

const PLANS = [
  { name: 'Starter', price: '$0' },
  { lit: true, name: 'Pro', price: '$250' },
];

// Label, Starter, Pro.
const COLUMNS =
  'grid grid-cols-[minmax(0,1fr)_6rem_8rem] items-center gap-3 px-4 sm:grid-cols-[minmax(0,1fr)_10rem_10rem]';

function Tick({ children }: { children: ReactNode }) {
  return (
    <li className="flex items-start gap-2">
      <Check
        aria-hidden="true"
        className="mt-[2px] size-4 shrink-0 text-clover"
        strokeWidth={2.25}
      />
      {children}
    </li>
  );
}

function Amount({ cell, strong }: { cell: Cell; strong?: boolean }) {
  return (
    <dd className="text-right leading-tight">
      <span
        className={`block ${strong ? 'font-medium text-head' : 'text-ink-2'}`}
      >
        {cell[0]}
      </span>
      {cell[1] && (
        <span className="block text-[12px] text-muted">{cell[1]}</span>
      )}
    </dd>
  );
}

/** One topic: a heading and a line, the table, then what's free as a checklist. */
function Block({ free, note, rows, title }: (typeof BLOCKS)[number]) {
  return (
    <div className="border-t border-rule pt-10">
      <h3 className="font-display text-[20px] font-medium text-head">
        {title}
      </h3>
      <p className="mt-2 max-w-[34rem] text-[14px] text-ink-2">{note}</p>
      <div className="mt-6 overflow-hidden rounded-lg border border-rule bg-raised">
        <div
          className={`${COLUMNS} border-b border-rule py-2 text-[12px] text-muted`}
        >
          <span />
          <span className="text-right">Starter</span>
          <span className="text-right">Pro</span>
        </div>
        <dl className="divide-y divide-rule text-[13px]">
          {rows.map((r) => (
            <div className={`${COLUMNS} min-h-[40px] py-2`} key={r.label}>
              <dt
                className={`flex items-center gap-2 ${r.extra ? 'text-ink-2' : 'font-medium text-head'}`}
              >
                {r.extra && (
                  <span className="grid size-4 shrink-0 place-items-center rounded-full bg-white/10">
                    <Plus aria-hidden="true" className="size-2.5 text-muted" />
                  </span>
                )}
                {r.label}
              </dt>
              <Amount cell={r.cells[0]} strong={!r.extra} />
              <Amount cell={r.cells[1]} strong={!r.extra} />
            </div>
          ))}
        </dl>
      </div>
      {free.length > 0 && (
        <ul className="mt-5 grid grid-cols-1 gap-x-6 gap-y-2 text-[13px] text-ink-2 sm:grid-cols-3">
          {free.map((f) => (
            <Tick key={f}>{f}</Tick>
          ))}
        </ul>
      )}
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
        className={`${PAGE_WRAP} pt-16 md:pt-24 pb-20 md:pb-32`}
      >
        <div className="text-center">
          <h1 className="v4-h1">Pricing</h1>
          <p className="mt-5 v4-body text-ink-2">
            Every Boundary Cloud plan comes with a 60-day money-back guarantee.
          </p>
        </div>

        <div className="mx-auto mt-14 max-w-[840px] md:mt-20">
          {/* The free tier, wide, the way Better Stack opens with theirs. */}
          <div className="rounded-xl border border-rule bg-raised p-6">
            <div className="flex flex-wrap items-center justify-between gap-6">
              <div>
                <p className="text-[14px] font-medium text-head">
                  BAML, free and open source
                </p>
                <p className="mt-2 font-display text-[36px] leading-none font-medium text-head">
                  $0
                  <span className="ml-2 font-sans text-[13px] font-normal text-muted">
                    forever
                  </span>
                </p>
              </div>
              <Button href={LINKS.quickstart} variant="secondary">
                Install BAML
              </Button>
            </div>
            <ul className="mt-5 grid gap-x-6 gap-y-2 border-t border-rule pt-5 text-[13px] text-ink-2 sm:grid-cols-2">
              <Tick>The language, compiler and toolchain</Tick>
              <Tick>Runs on your machine</Tick>
              <Tick>Never calls our servers</Tick>
              <Tick>Source on GitHub</Tick>
            </ul>
          </div>

          <h2 className="mt-20 v4-h3 text-head">Boundary Cloud</h2>
          <p className="mt-2 text-[15px] text-ink-2">
            You pay for data and retention. Spans, seats, queries and profiles
            are free.
          </p>
          <div className="mt-6 grid gap-4 sm:grid-cols-2">
            {PLANS.map((p) => (
              <div
                className={`flex flex-wrap items-center justify-between gap-6 rounded-xl border p-6 ${p.lit ? 'border-clover/60 bg-clover/[0.07]' : 'border-rule bg-raised'}`}
                key={p.name}
              >
                <div>
                  <p className="text-[14px] font-medium text-head">{p.name}</p>
                  <p className="mt-2 font-display text-[36px] leading-none font-medium text-head tabular-nums">
                    {p.price}
                  </p>
                  <p className="mt-1 text-[12px] text-muted">per month</p>
                </div>
                <Button href={BOOK} variant={p.lit ? 'primary' : 'secondary'}>
                  Get early access
                </Button>
              </div>
            ))}
          </div>

          <div className="mt-14 space-y-12">
            {BLOCKS.map((b) => (
              <Block key={b.title} {...b} />
            ))}
          </div>
        </div>
      </section>
    </PageShell>
  );
}
