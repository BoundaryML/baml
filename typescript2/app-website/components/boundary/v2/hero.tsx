import { cta } from '../analytics';
import BoardDemo from '../board/board-demo';
import { ParallaxImage } from '../motion';
import { BOOK_MEETING } from '../page-parts';
import Flow from './flow';

export default function Hero() {
  return (
    <section className="overflow-hidden">
      <div className="max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-24 text-center">
        <h1 className="v2-h1 v2-painted pb-[0.08em]">
          Agentic
          <br />
          Observability
        </h1>
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

      {/* The product itself, edge to edge on the painting. */}
      <div className="relative overflow-hidden bg-pasture p-4 sm:p-[5%] lg:px-[10%] lg:py-[6%]">
        <ParallaxImage sizes="100vw" src="/boundary/painting.jpg" />
        <div className="relative max-w-[1344px] mx-auto rounded-[12px] overflow-hidden border border-white/15 bg-[#121315] shadow-[0_40px_120px_-20px_rgb(0_0_0/0.8)]">
          <div className="max-md:aspect-[701/482] max-md:overflow-hidden">
            <div className="max-md:w-[244.22%] max-md:ml-[-40.51%] max-md:mt-[-27.39%]">
              <BoardDemo initialTheme="dark" />
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
