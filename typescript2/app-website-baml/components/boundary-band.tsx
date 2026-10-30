import comboWhite from '@boundaryml/brand/logo/svg/combination-mark-white.svg';
import Image from 'next/image';
import { section } from './analytics';
import Reveal from './reveal';
import { Arrow, LINKS, WRAP } from './ui';

// Where Boundary is introduced, and the one place green appears: Boundary's band, on the daylight hills painting, under the combination mark.
// Layers follow Boundary's stack diagram, with placeholder labels like "Your bucket" left out.
/* Name and description sit on one line from sm up. On a phone there isn't room for both, and the
   description was setting as two right-aligned, ragged-left lines against a name that had itself
   wrapped — so the row stacks instead, left-aligned, the way it reads. */
const ROW =
  'flex max-sm:flex-col max-sm:items-start items-center justify-between gap-4 max-sm:gap-1 px-5 py-4';
const DESC = 'text-right max-sm:text-left';
const NAME =
  'font-display font-medium text-[17px] leading-[22px] tracking-[-0.02em]';

export default function BoundaryBand() {
  return (
    <section
      {...section('boundary_band')}
      aria-labelledby="boundary-title"
      className={`${WRAP} pt-8`}
    >
      <div className="relative rounded-2xl overflow-hidden p-6 sm:p-10 md:p-14 flex flex-col lg:flex-row lg:items-center gap-12 lg:gap-14">
        {/* The band is inset by the page gutters and capped at the 1440 grid, so it is never 100vw.
            Spelling that out in `sizes` is worth ~0.8MB: it drops the phone candidate from 828px to
            640 and the desktop one from 3840 to 1920. */}
        <Image
          alt=""
          className="object-cover object-[50%_60%]"
          fill
          quality={90}
          sizes="(min-width: 1440px) 1344px, (min-width: 768px) calc(100vw - 96px), calc(100vw - 40px)"
          src="/paint/boundary-day-hills.jpg"
        />
        <div className="absolute inset-0 bg-[rgb(18_40_14/0.5)] lg:bg-transparent lg:bg-[linear-gradient(90deg,rgb(18_40_14/0.62)_0%,rgb(18_40_14/0.35)_40%,rgb(18_40_14/0)_60%)]" />

        <div className="relative lg:w-[460px] shrink-0 flex flex-col gap-[22px] text-white">
          <Image
            alt=""
            className="w-[72px] sm:w-[84px] h-auto"
            height={90}
            src={comboWhite}
            width={84}
          />
          <h2
            className="font-display font-medium text-[clamp(28px,2.4vw,34px)] leading-[1.18] tracking-[-0.03em]"
            id="boundary-title"
          >
            Query locally, or in the cloud.
          </h2>
          <p className="text-base leading-[25px] text-white/85">
            BAML records every run to files in your project, and{' '}
            <code className="font-mono text-[0.94em]">baml query</code> reads
            them for free. In production, send the recordings to Boundary,
            hosted for you or in your own AWS account, and build dashboards,
            metrics, and alerts on top.
          </p>
          <a
            className="group self-start inline-flex items-center gap-1 px-[18px] h-11 rounded-lg bg-white text-[#0f0f0f] font-medium text-[15px]"
            href={LINKS.boundary}
          >
            Explore Boundary <Arrow />
          </a>
        </div>

        <Reveal className="relative flex-1 min-w-0">
          <figure
            aria-label="The Boundary stack"
            className="flex flex-col text-[13px] leading-4"
          >
            <div className="rounded-[10px] overflow-hidden shadow-[0_1px_2px_rgb(20_40_20/0.15)]">
              <div className={`${ROW} bg-white text-[#555]`}>
                <span className={`${NAME} text-[#0f0f0f]`}>Studio</span>
                <span className={DESC}>
                  span tree and profiler, agent-built dashboards
                </span>
              </div>
              <div
                className={`${ROW} bg-white text-[#555] border-t border-[#e6e6e3]`}
              >
                <span className={`${NAME} text-[#0f0f0f]`}>Query</span>
                <span className={DESC}>
                  SQL over every call, from Studio, baml query, curl or an agent
                </span>
              </div>
              <div className={`${ROW} bg-pasture-800 text-white/75`}>
                <span className={`${NAME} text-white`}>Your data</span>
                <span className={DESC}>hosted by Boundary or in your AWS</span>
              </div>
            </div>
            <span className="px-5 py-3.5 font-mono text-xs text-white/85">
              ↑ every call recorded
            </span>
            <div
              className={`${ROW} rounded-[10px] bg-clover-900 border border-clover/30 text-[#ababa6]`}
            >
              <span className={`${NAME} text-[#f4f4f2]`}>BAML</span>
              <span className={DESC}>
                free, on your machine, no tracing SDK
              </span>
            </div>
          </figure>
        </Reveal>
      </div>
    </section>
  );
}
