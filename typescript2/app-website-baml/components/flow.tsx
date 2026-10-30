import { section } from './analytics';
import Reveal from './reveal';
import { WRAP } from './ui';

/* What the hero's sentence describes, left to right: the program runs, the runtime records it, SQL
   reads the record, and the tools are built on the result. */
const STEPS: { title: string; body: string; mono?: boolean }[] = [
  { body: 'Agents and AI functions, written in BAML.', title: 'Your program' },
  { body: 'Every execution, by the runtime itself.', title: 'Recorded' },
  { body: 'SQL, locally or in the cloud.', mono: true, title: 'baml query' },
  { body: 'Evals, cost alerts, error tracking, and more.', title: 'Anything' },
];

export default function Flow() {
  return (
    <section
      {...section('flow')}
      aria-label="How BAML records and queries your program"
      className={`${WRAP} pb-12 md:pb-16`}
    >
      <Reveal>
        <ol className="grid sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {STEPS.map((s, i) => (
            <li
              className={`relative flex flex-col gap-1.5 p-5 rounded-xl border bg-raised ${i === 0 ? 'border-rule-accent' : 'border-rule'}`}
              key={s.title}
            >
              <span
                className={
                  s.mono
                    ? 'font-mono font-medium text-[15px] leading-6'
                    : 'font-display font-medium text-[17px] leading-6 tracking-[-0.02em]'
                }
              >
                {s.title}
              </span>
              <span className="text-sm leading-[21px] text-body">{s.body}</span>
              {i < STEPS.length - 1 && (
                <span
                  aria-hidden
                  className="max-lg:hidden absolute top-1/2 -right-[13px] -translate-y-1/2 z-10 text-accent"
                >
                  →
                </span>
              )}
            </li>
          ))}
        </ol>
      </Reveal>
    </section>
  );
}
