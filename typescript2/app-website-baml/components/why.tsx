import { section } from './analytics';
import Reveal from './reveal';
import { LANES, LEFT, PAD_Y, WRAP } from './ui';
import WhyField from './why-field';

// boundaryml.com's "Why a new language?" argument, compressed to one statement and one consequence.
export default function Why() {
  return (
    <section
      {...section('why')}
      aria-labelledby="why-title"
      className={`${WRAP} ${PAD_Y} ${LANES}`}
    >
      <div className={`${LEFT} flex flex-col gap-4`}>
        <h2 className="h2" id="why-title">
          Why a new language?
        </h2>
        {/* The glyph field fills the column under the heading. Two columns only: once the lanes
            stack there is no empty column to fill, so below lg it isn't rendered at all. */}
        <div
          aria-hidden
          className="relative flex-1 min-h-0 mt-6 max-lg:hidden pointer-events-none"
        >
          <WhyField />
        </div>
      </div>
      <Reveal className="flex-1 flex flex-col gap-6 max-w-[720px]">
        <p className="font-display text-[clamp(24px,2.1vw,30px)] leading-[1.33] tracking-[-0.025em] text-pretty">
          Assembly was built for hardware, Java for operating systems,
          JavaScript for the web. Each traded soundness for human speed. Agents
          will use every escape hatch you give them.
        </p>
        <p className="text-lg leading-[29px] text-body text-pretty">
          So BAML makes invalid states hard to represent, keeps changes local,
          exposes nondeterminism instead of hiding it, and gives agents
          structured ways to inspect the code they&apos;re changing.
        </p>
      </Reveal>
    </section>
  );
}
