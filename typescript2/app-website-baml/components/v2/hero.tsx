import { section } from '../analytics';
import { Button, LINKS } from '../ui';
import WhyField from '../why-field';
import Flow from './flow';
import { EXPLORE } from './links';
import VersionBadge from './version-badge';

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
      <div className="relative max-w-[1440px] mx-auto px-5 md:px-12 pt-14 md:pt-24 pb-14 md:pb-24">
        <VersionBadge />
        <h1 className="v2-h1 max-w-[12em] mx-auto text-center" id="hero-title">
          The programming language for agents.
        </h1>
        <p className="mt-8 md:mt-10 text-center text-[15px] md:text-lg leading-[1.5] tracking-[-0.01em] text-white/60">
          <strong className="font-medium text-white">Rust</strong>-like types ·{' '}
          <strong className="font-medium text-white">Go</strong>-like compile
          times · <strong className="font-medium text-white">SQL</strong>
          -queryable telemetry
        </p>

        <p className="max-w-160 mx-auto mt-6 md:mt-8 text-center text-lg md:text-xl leading-[1.5] tracking-[-0.01em] text-white/70">
          BAML is a language for writing AI agents, with efficient built-in
          telemetry. BAML builds a database of your program&apos;s executions
          that you can query locally (or in the cloud) to build anything: evals,
          cost alerts, error tracking, and more.
        </p>
        <div className="flex flex-wrap justify-center gap-2.5 mt-8">
          <Button href={LINKS.quickstart}>Install BAML</Button>
          <Button href={EXPLORE} variant="secondary">
            Explore BAML
          </Button>
        </div>

        <div className="mt-14 md:mt-24">
          <Flow />
        </div>
      </div>
    </section>
  );
}
