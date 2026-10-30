import comboWhite from '@boundaryml/brand/logo/svg/combination-mark-white.svg';
import Image from 'next/image';
import { section } from '../analytics';
import { Arrow, LINKS } from '../ui';

// One query, two places it can run. Green signal, because this is where Boundary comes in.
const QUERY = 'M210 140 C280 140 280 62 350 62';
const CLOUD = 'M210 140 C280 140 280 218 350 218';

export default function BoundaryBand() {
  return (
    <section
      {...section('boundary_band')}
      aria-labelledby="boundary-title"
      className="relative overflow-hidden scroll-mt-16"
    >
      <Image
        alt=""
        className="object-cover object-[50%_60%]"
        fill
        quality={90}
        sizes="100vw"
        src="/paint/boundary-day-hills.jpg"
      />
      <div className="absolute inset-0 bg-[rgb(10_24_8/0.62)]" />
      <div className="relative max-w-[1440px] mx-auto px-5 md:px-12 py-20 md:py-36 grid lg:grid-cols-2 gap-12 lg:gap-16 items-center">
        <div className="flex flex-col gap-6">
          <Image
            alt=""
            className="w-[84px] h-auto"
            height={90}
            src={comboWhite}
            width={84}
          />
          <h2 className="v2-h2" id="boundary-title">
            Query locally, or in the cloud.
          </h2>
          <p className="max-w-140 text-xl leading-[1.5] text-white/85">
            BAML records every run to files in your project, and{' '}
            <code className="font-mono text-[0.92em]">baml query</code> reads
            them for free. In production, send the recordings to Boundary,
            hosted for you or in your own AWS account, and build dashboards,
            metrics, and alerts on top.
          </p>
          <a
            className="group self-start inline-flex items-center gap-1 h-12 px-6 rounded-full bg-white text-black font-medium text-[15px]"
            href={LINKS.boundary}
          >
            Explore Boundary <Arrow />
          </a>
        </div>
        <svg
          aria-label="The same baml query runs on files in your project or in Boundary."
          className="w-full h-auto"
          role="img"
          viewBox="0 0 600 280"
        >
          {[QUERY, CLOUD].map((d, i) => (
            <g className="v2-pulse-green" key={d}>
              <path
                className="v2-wire"
                d={d}
                pathLength={1000}
                style={{ stroke: 'rgb(255 255 255 / 0.4)' }}
              />
              <path
                className="v2-pulse"
                d={d}
                pathLength={1000}
                style={{ '--delay': `${-1.8 * i}s` } as React.CSSProperties}
              />
            </g>
          ))}
          <rect
            fill="#000"
            height={72}
            rx={12}
            stroke="rgb(255 255 255 / 0.3)"
            width={210}
            y={104}
          />
          <text className="font-mono" fill="#fff" fontSize={20} x={28} y={147}>
            baml query
          </text>
          <rect
            fill="var(--color-clover-900)"
            height={84}
            rx={12}
            stroke="var(--color-clover-core)"
            width={250}
            x={350}
            y={20}
          />
          <text fill="#fff" fontSize={19} x={372} y={54}>
            Your laptop
          </text>
          <text fill="rgb(255 255 255 / 0.7)" fontSize={15} x={372} y={80}>
            files in your project
          </text>
          <rect
            fill="var(--color-pasture-800)"
            height={84}
            rx={12}
            stroke="var(--color-pasture-300)"
            width={250}
            x={350}
            y={176}
          />
          <text fill="#fff" fontSize={19} x={372} y={210}>
            Boundary
          </text>
          <text fill="rgb(255 255 255 / 0.7)" fontSize={15} x={372} y={236}>
            hosted, or in your AWS
          </text>
        </svg>
      </div>
    </section>
  );
}
