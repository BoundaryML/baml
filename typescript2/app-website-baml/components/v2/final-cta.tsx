import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { section } from '../analytics';
import { Button, LINKS, PAD_Y, WRAP } from '../ui';
import Install from '../v1/install';
import { EXPLORE } from './links';

/* The leaping pixel lamb over the clover field.
   The closing CTA repeats the hero's two actions, with the install panel included directly rather
   than only linked, so "hand the quickstart to your agent" becomes a prompt you can copy here. */
export default function FinalCta() {
  return (
    <section
      {...section('final_cta')}
      aria-labelledby="cta-title"
      className="on-paint relative overflow-hidden"
    >
      <Image
        alt=""
        className="object-cover object-[50%_70%]"
        fill
        quality={90}
        sizes="100vw"
        src="/paint/baml-dusk-field.jpg"
      />
      {/* Flat under lg, where the copy spans the whole band and the sky would otherwise sit behind
          the headline; the sideways gradient only makes sense once the lamb has its own column. */}
      <div className="absolute inset-0 bg-[rgb(var(--scrim)/0.72)] lg:bg-transparent lg:bg-[linear-gradient(90deg,rgb(var(--scrim)/0.78)_0%,rgb(var(--scrim)/0.3)_55%,rgb(var(--scrim)/0)_100%)]" />
      <div
        className={`relative ${WRAP} ${PAD_Y} lg:pr-24 flex max-lg:flex-col max-lg:items-start items-center justify-between gap-12 max-lg:gap-8`}
      >
        <div className="w-full max-w-[560px] flex flex-col gap-8">
          <h2 className="v2-h2" id="cta-title">
            Start using BAML.
          </h2>
          <p className="text-lg leading-7 text-white/85">
            Install the toolchain, add the agent guidance, and run your first
            function. Or copy the prompt below and hand it to your agent.
          </p>
          <div className="flex flex-wrap gap-2.5">
            <Button href={LINKS.quickstart}>Install BAML</Button>
            <Button href={EXPLORE} variant="onPainting">
              Explore BAML
            </Button>
          </div>
          <Install />
        </div>
        {/* Below lg the mark stacks above the copy rather than dropping out, at the size the
            Boundary band uses for its combination mark. */}
        <Image
          alt=""
          className="shrink-0 w-[88px] sm:w-[104px] lg:w-[clamp(150px,15vw,220px)] h-auto max-lg:order-first"
          height={285}
          src={lambWhite}
          width={240}
        />
      </div>
    </section>
  );
}
