import { section } from '../analytics';
import { Button, LINKS } from '../ui';
import Editor from '../v1/editor';
import Install from '../v1/install';
import WhyField from '../why-field';
import Flow from './flow';
import { EXPLORE } from './links';

export default function Hero() {
  return (
    <section
      {...section('hero')}
      aria-labelledby="hero-title"
      className="relative overflow-hidden"
    >
      {/* Noise settling into a typed program, once, behind the headline. */}
      <div
        aria-hidden
        className="absolute inset-x-0 top-0 h-[70vh] opacity-70 pointer-events-none"
      >
        <WhyField />
      </div>
      <div className="relative max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-24 pb-14 md:pb-24">
        <h1 className="v2-h1 max-w-[12em]" id="hero-title">
          The programming language for agents.
        </h1>

        <div className="grid xl:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] gap-10 xl:gap-16 mt-10 md:mt-16">
          <div className="flex flex-col gap-7">
            <p className="max-w-150 text-xl leading-[1.5] tracking-[-0.01em] text-white/75">
              BAML is a language for writing AI agents, with efficient built-in
              telemetry. BAML builds a database of your program&apos;s
              executions that you can query locally (or in the cloud) to build
              anything: evals, cost alerts, error tracking, and more.
            </p>
            <div className="flex flex-wrap gap-2.5">
              <Button href={LINKS.quickstart}>Install BAML</Button>
              <Button href={EXPLORE} variant="secondary">
                Explore BAML
              </Button>
            </div>
            <div className="xl:mt-auto">
              <Install />
            </div>
          </div>
          <div className="on-paint min-w-0 rounded-2xl border border-clover/25 shadow-[0_0_120px_-30px_var(--color-clover-core)]">
            <Editor />
          </div>
        </div>

        <div className="mt-14 md:mt-24">
          <Flow />
        </div>
      </div>
    </section>
  );
}
