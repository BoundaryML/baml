import type { Metadata } from 'next';
import { section } from '@/components/analytics';
import Footer from '@/components/footer';
import Nav from '@/components/nav';
import PageHeader from '@/components/page-header';
import { SectionViews } from '@/components/track';
import { Button, LINKS, PAD_Y, WRAP } from '@/components/ui';

// What boundaryml.com/pricing said (typescript2/app-website's content/pricing.md), in this site's
// design: the free local toolchain and the hosted plan that's coming, side by side.

export const metadata: Metadata = {
  alternates: { canonical: '/pricing' },
  description:
    'BAML is free and open source. It runs entirely on your machine and never calls our servers.',
  title: 'Pricing · BAML',
};

const PLANS = [
  {
    action: { href: LINKS.quickstart, label: 'Install BAML' },
    body: 'The language and the local toolchain. BAML runs on your machine and never calls Boundary’s servers.',
    features: [
      'Write and run BAML programs',
      'Generate typed SDKs for your app',
      'The local playground',
      'Fit it into an existing codebase',
    ],
    name: 'Local BAML',
    note: 'Apache 2.0, free forever',
    price: 'Free',
  },
  {
    action: { href: LINKS.discord, label: 'Ask on Discord' },
    body: 'Hosted capabilities on top of local BAML. It is never required to run BAML, and its pricing isn’t published yet.',
    features: ['Observability', 'Team controls', 'Governance'],
    name: 'BAML Cloud',
    note: 'For availability or early access',
    price: 'Later this year',
  },
] as const;

export default function Pricing() {
  return (
    <>
      <Nav />
      <main className="flex-1" id="main">
        <PageHeader
          lead="It runs entirely on your machine. It never calls our servers."
          title="BAML is free and open source."
        >
          <Button href={LINKS.quickstart}>Install BAML</Button>
          <Button href={LINKS.github} variant="secondary">
            Source on GitHub
          </Button>
        </PageHeader>

        <section
          {...section('plans')}
          aria-label="Plans"
          className={`${WRAP} ${PAD_Y}`}
        >
          <ul className="grid md:grid-cols-2 gap-4">
            {PLANS.map((p, i) => (
              <li
                className={`flex flex-col gap-8 p-7 md:p-9 rounded-xl border ${
                  i === 0 ? 'border-rule-accent bg-raised' : 'border-rule'
                }`}
                key={p.name}
              >
                <div className="flex flex-col gap-3">
                  <span className="text-[15px] font-medium text-ink">
                    {p.name}
                  </span>
                  <span className="font-display font-medium text-[clamp(36px,3.4vw,48px)] leading-[1.1] tracking-[-0.04em] text-head">
                    {p.price}
                  </span>
                  <p className="max-w-[440px] text-[15px] leading-6 text-body">
                    {p.body}
                  </p>
                </div>
                <ul className="flex flex-col gap-3 pt-6 border-t border-rule text-[15px] leading-6 text-ink-2">
                  {p.features.map((f) => (
                    <li className="flex gap-3" key={f}>
                      <span aria-hidden className="text-accent">
                        ✓
                      </span>
                      {f}
                    </li>
                  ))}
                </ul>
                <div className="mt-auto flex flex-col gap-3">
                  <Button
                    href={p.action.href}
                    variant={i === 0 ? 'primary' : 'secondary'}
                  >
                    {p.action.label}
                  </Button>
                  <span className="text-[13px] text-muted">{p.note}</span>
                </div>
              </li>
            ))}
          </ul>
        </section>
      </main>
      <Footer />
      <SectionViews />
    </>
  );
}
