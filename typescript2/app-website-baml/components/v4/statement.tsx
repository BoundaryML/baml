import { section } from '../analytics';

/** Section 2: why BAML is a language, from the founders, beside a blueprint of three Raptor engines. */
export default function Statement() {
  return (
    <section
      {...section('why')}
      aria-labelledby="why"
      className="border-t border-white/[0.08]"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28 grid lg:grid-cols-[minmax(0,1fr)_minmax(0,1.05fr)] gap-x-16 gap-y-12 items-center">
        <div>
          <h2 className="v4-h2 max-w-[18em] text-balance" id="why">
            Building a fast iteration loop is simple when everything speaks the
            same language.
          </h2>
          <p className="mt-8 font-display text-[24px] md:text-[30px] leading-[1.2] tracking-[-0.02em] text-white">
            We didn't want to build a programming language.
          </p>
          <div className="mt-6 flex flex-col gap-5 max-w-[36rem] text-[17px] md:text-lg leading-[1.55] text-white/70">
            <p>
              But agents today struggle waiting for CI times, wrangling many
              different tools to run experiments, check alerts, build dashboards
              or figure out what version of the program did something.
            </p>
            <p>
              To make something 100x better, you have to start from scratch. We
              built a virtual machine that can run anywhere. A compiler that
              knows what errors you're running into in production, and a data
              lake of all your program's data that knows what type definitions
              your function was using.
            </p>
          </div>
        </div>
        <figure className="v3-blueprint-sheet rounded-2xl overflow-hidden border border-white/10">
          {/* biome-ignore lint/performance/noImgElement: a static SVG drawing, shown at its own size */}
          <img
            alt="Raptor 1, Raptor 2, and Raptor 3 engines side by side, drawn as a blueprint. Each one has fewer external parts than the last."
            className="block w-full h-auto"
            height={720}
            src="/v4/raptors.svg"
            width={960}
          />
        </figure>
      </div>
    </section>
  );
}
