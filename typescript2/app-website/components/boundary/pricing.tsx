import { cta } from './analytics';
import { Rise } from './motion';

const DEMO = 'https://cal.com/boundaryml/30min';

// `event` names the CTA in PostHog (see ./analytics.ts). Sign-up has no URL yet, so its two CTAs stay on "#".
const TIERS = [
  {
    cta: 'Start free',
    event: 'start_free',
    href: '#',
    name: 'Free',
    per: '/ month',
    price: '$0',
    style: 'plain',
  },
  {
    cta: 'Start a trial',
    event: 'start_trial',
    href: '#',
    name: 'Team',
    per: '/ month',
    price: '$00',
    style: 'featured',
  },
  {
    cta: 'Contact sales',
    event: 'contact_sales',
    href: DEMO,
    name: 'Enterprise',
    per: '',
    price: 'Custom',
    style: 'dark-cta',
  },
] as const;

const FEATURES = [
  'Lorem ipsum dolor sit',
  'Consectetur adipiscing',
  'Sed do eiusmod tempor',
];

export function Pricing() {
  return (
    <section
      className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-30 pb-20 md:pb-34 border-t border-hair"
      id="pricing"
    >
      <div className="flex flex-col sm:flex-row sm:justify-between sm:items-end gap-6 mb-10 md:mb-14">
        <Rise className="h2-display" text="Start free. Grow with Boundary." />
        <a
          // biome-ignore lint/a11y/useValidAnchor: the full pricing page doesn't exist yet
          href="#"
          {...cta('full_pricing', 'pricing')}
          className="text-[15px] leading-5 font-medium text-ink ulink hover:text-accent"
        >
          See full pricing
        </a>
      </div>
      <div className="grid md:grid-cols-3 gap-4">
        {TIERS.map((t) => {
          const featured = t.style === 'featured';
          return (
            <div
              className={`flex flex-col gap-8 p-8 rounded-xl border ${featured ? 'bg-pasture border-pasture text-white' : 'bg-surface border-card-line text-ink'}`}
              key={t.name}
            >
              <div className="flex flex-col gap-3">
                <span className="text-base font-medium">{t.name}</span>
                <span className="flex items-baseline gap-1.5">
                  <span className="font-display text-5xl leading-13 tracking-[-0.04em]">
                    {t.price}
                  </span>
                  {t.per && (
                    <span
                      className={`text-sm ${featured ? 'text-white/70' : 'text-ink-2'}`}
                    >
                      {t.per}
                    </span>
                  )}
                </span>
                <p
                  className={`text-[15px] leading-[23px] ${featured ? 'text-white/70' : 'text-ink-2'}`}
                >
                  Lorem ipsum dolor sit amet, consectetur adipiscing.
                </p>
              </div>
              <ul
                className={`flex flex-col gap-3 pt-6 border-t text-sm ${featured ? 'border-white/18' : 'border-hair text-ink'}`}
              >
                {FEATURES.map((f) => (
                  <li key={f}>✓&nbsp;&nbsp;{f}</li>
                ))}
              </ul>
              <a
                href={t.href}
                {...cta(t.event, 'pricing')}
                className={`mt-auto py-3.25 rounded-md text-center text-[15px] leading-4.5 font-medium active:translate-y-px transition ${
                  featured
                    ? 'bg-white text-[#0f0f0f] hover:bg-pasture-100'
                    : t.style === 'dark-cta'
                      ? 'bg-ink text-paper hover:opacity-85'
                      : 'border border-line text-ink hover:border-ink-3'
                }`}
              >
                {t.cta}
              </a>
            </div>
          );
        })}
      </div>
    </section>
  );
}

export function Enterprise() {
  return (
    <section className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pb-20 md:pb-34">
      <div className="flex flex-col lg:flex-row lg:justify-between lg:items-center gap-12 lg:gap-16 p-8 md:p-14 rounded-[14px] bg-panel">
        <div className="lg:w-140 flex flex-col gap-6">
          <Rise
            className="h2-display !text-[clamp(28px,2.6vw,38px)]"
            text="Private values never leave your data plane."
          />
          <div className="flex items-center gap-5">
            <a
              href={DEMO}
              {...cta('contact_sales', 'enterprise')}
              className="py-3.25 px-5 rounded-md bg-ink text-paper text-[15px] leading-4.5 font-medium hover:opacity-85 active:translate-y-px transition"
            >
              Contact sales
            </a>
            <a
              href={DEMO}
              {...cta('book_demo', 'enterprise')}
              className="text-[15px] leading-5 text-ink ulink hover:text-accent"
            >
              or book a demo
            </a>
          </div>
        </div>
        <ul className="lg:w-130 border-t border-card-line">
          {[
            ['Your data, your cloud', 'hosted by Boundary or in your AWS'],
            [
              'Access',
              'SQL from Studio, the CLI, curl, or an agent with a read key',
            ],
            [
              'Capture',
              'profiling on every call; inputs and outputs when you opt in',
            ],
          ].map(([p, d]) => (
            <li
              className="flex justify-between gap-6 py-4.5 border-b border-card-line"
              key={p}
            >
              <span className="shrink-0 text-[15px] leading-5 text-ink">
                {p}
              </span>
              <span className="text-sm leading-5 text-ink-2 text-right">
                {d}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
