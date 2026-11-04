import comboWhite from '@boundaryml/brand/logo/svg/combination-mark-white.svg';
import Image from 'next/image';
import { section } from '../analytics';
import { Arrow, LINKS } from '../ui';
import Places from './places';

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
        <Places />
      </div>
    </section>
  );
}
