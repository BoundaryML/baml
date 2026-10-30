import type { ReactNode } from 'react';

// Four steps on one wire, with a small picture of what each one looks like.
function Snippet({ children }: { children: ReactNode }) {
  return (
    <pre className="overflow-x-auto font-mono text-[12px] leading-[1.75] text-white/80 whitespace-pre-wrap break-words">
      {children}
    </pre>
  );
}

const STEPS: { title: string; body: string; picture: ReactNode }[] = [
  {
    body: "Point your existing OTLP exporter at Boundary. Book a meeting, and we'll set it up with you.",
    picture: (
      <Snippet>
        <span className="text-clover">OTEL_EXPORTER_OTLP_ENDPOINT</span>=
        <span className="text-pasture-200">your-boundary-endpoint</span>
      </Snippet>
    ),
    title: 'Send your traces',
  },
  {
    body: 'Run SQL over every call from Studio, the baml query CLI, or HTTP.',
    picture: (
      <Snippet>
        <span className="text-clover">SELECT</span> span_name, count(*){'\n'}
        <span className="text-clover">FROM</span> spans
      </Snippet>
    ),
    title: 'Query them with SQL',
  },
  {
    body: 'Turn a query into a dashboard in Studio, or have your coding agent wire it into Slack, GitHub Actions, or your admin panel.',
    picture: (
      <svg
        aria-hidden="true"
        className="w-full h-14"
        fill="none"
        viewBox="0 0 240 56"
      >
        <path
          d="M0 46 L30 40 L60 44 L90 28 L120 32 L150 18 L180 24 L210 8 L240 12"
          stroke="var(--color-pasture-200)"
          strokeWidth="2"
        />
      </svg>
    ),
    title: 'Build your tools',
  },
  {
    body: 'Move one AI function into BAML. Each call then records its typed result, the step that failed, and the code version.',
    picture: (
      <Snippet>
        <span className="text-clover">function</span> ExtractInvoice(file: pdf)
        {'\n'}
        {'  '}-&gt; <span className="text-pasture-200">Invoice</span>
      </Snippet>
    ),
    title: 'Add BAML for richer data',
  },
];

export default function GetStarted() {
  return (
    <section className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36">
      <h2 className="v2-h2">Get started in minutes.</h2>
      <p className="max-w-130 mt-5 text-xl leading-[1.5] text-white/70">
        Start with the traces you already have, and add BAML when you want
        richer data.
      </p>

      {/* The wire runs across the top of the steps on desktop. */}
      <svg
        aria-hidden="true"
        className="hidden lg:block w-full h-3 mt-14"
        preserveAspectRatio="none"
        viewBox="0 0 1000 12"
      >
        <path className="v2-wire" d="M0 6H1000" pathLength={1000} />
        <path className="v2-pulse" d="M0 6H1000" pathLength={1000} />
      </svg>
      <ol className="grid sm:grid-cols-2 lg:grid-cols-4 gap-px mt-10 lg:mt-3 rounded-2xl overflow-hidden bg-white/12 border border-white/12">
        {STEPS.map((s, i) => (
          <li className="flex flex-col gap-4 p-6 md:p-7 bg-black" key={s.title}>
            <span className="grid place-items-center w-7 h-7 rounded-full border border-white/25 text-[13px]">
              {i + 1}
            </span>
            <h3 className="font-display font-medium text-[22px] leading-[1.2] tracking-[-0.03em]">
              {s.title}
            </h3>
            <p className="text-[15px] leading-6 text-white/65">{s.body}</p>
            <div className="mt-auto pt-5 border-t border-white/10 min-h-22">
              {s.picture}
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
