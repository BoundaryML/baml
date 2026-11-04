import { cta } from '../analytics';
import { ParallaxImage } from '../motion';
import { BOOK_MEETING } from '../page-parts';
import Dashboards from './dashboards';
import Flow from './flow';
import PaintedHeadline from './painted-headline';

export default function Hero() {
  return (
    <section className="overflow-hidden">
      <div className="max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-24 text-center">
        <PaintedHeadline>
          Agentic
          <br />
          Observability
        </PaintedHeadline>
        <p className="max-w-160 mx-auto mt-7 md:mt-9 text-lg md:text-xl leading-[1.5] tracking-[-0.01em] text-white/70">
          Query OTEL traces to create alerts, dashboards, AI evals, or build
          your own tooling. Write your AI workflows in the BAML language to
          record richer, but cheaper telemetry data built for agents.
        </p>
        <div className="flex justify-center gap-2.5 mt-8 text-[15px] leading-4.5 font-medium">
          <a
            href={BOOK_MEETING}
            {...cta('book_meeting', 'hero')}
            className="focus-ring py-3.5 px-6 rounded-full bg-white text-black hover:bg-pasture-100 active:translate-y-px transition"
          >
            Book a meeting
          </a>
          <a
            // biome-ignore lint/a11y/useValidAnchor: sign-up has no URL yet
            href="#"
            {...cta('start_free', 'hero')}
            className="py-3.5 px-6 rounded-full border border-white/20 hover:border-white/60 active:translate-y-px transition"
          >
            Start free
          </a>
        </div>
      </div>

      <div className="max-w-[1440px] mx-auto px-5 md:px-12 pt-14 md:pt-20 pb-14 md:pb-20">
        <Flow />
      </div>

      {/* Custom dashboards, on the painting, running off its bottom edge. */}
      <div className="relative overflow-hidden bg-pasture px-4 pt-8 sm:px-[5%] sm:pt-12 lg:px-[8%] lg:pt-16">
        <ParallaxImage sizes="100vw" src="/boundary/painting.jpg" />
        <Dashboards />
      </div>
    </section>
  );
}
