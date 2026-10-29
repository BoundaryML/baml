import Image from 'next/image';
import type { CSSProperties } from 'react';
import { section } from './analytics';
import Editor from './editor';
import Install from './install';
import { Button, LINKS, WRAP } from './ui';

const delay = (ms: number) => ({ '--d': `${ms}ms` }) as CSSProperties;

export default function Hero() {
  // The two-column hero turns on at xl, not lg. The copy holds a fixed 640px lane, and at 1024 that
  // left the painting 240px — the editor was a sliver of wrapped code beside two thirds of an empty
  // column. 640 + the gap + a usable editor needs about 1180, so 1024 stacks instead.
  return (
    <section
      {...section('hero')}
      aria-labelledby="hero-title"
      className={`${WRAP} pt-14 xl:pt-[88px] pb-14 xl:pb-20 flex flex-col xl:flex-row gap-12 items-stretch`}
    >
      <div className="xl:w-[640px] shrink-0 flex flex-col gap-8">
        {/* The live site's "The programming language for agents." is in the footer instead, so the
            page doesn't open and close on the same line.
            Below 640 the headline is sized off the viewport rather than held at the desktop clamp's
            floor: 42px runs to five lines at 360, which is a paragraph, not a headline. */}
        <h1
          className="rise font-medium text-head text-[clamp(42px,4.6vw,66px)] max-sm:text-[clamp(34px,9.6vw,42px)] leading-[1.03] tracking-[-0.045em] text-balance"
          id="hero-title"
        >
          Modern programming languages weren&apos;t built for agents.
        </h1>
        <p
          className="rise max-w-[540px] text-lg leading-[29px] text-body"
          style={delay(120)}
        >
          BAML is a Turing-complete language for software written by agents and
          humans: TypeScript-like syntax, types that hold at runtime, explicit
          errors.
        </p>
        <div className="rise flex flex-wrap gap-2.5" style={delay(200)}>
          <Button href={LINKS.quickstart}>Install BAML</Button>
          <Button href={LINKS.explore} variant="secondary">
            Explore BAML
          </Button>
        </div>
        <div className="rise xl:mt-auto" style={delay(280)}>
          <Install />
        </div>
      </div>

      {/* Below sm the painting runs full-bleed, like the toolchain and closing bands: inset, it is a
          12px border around the editor rather than a picture, and it costs the editor 24px of width. */}
      <div className="on-paint relative flex-1 min-w-0 rounded-2xl overflow-hidden p-3 sm:p-10 xl:min-h-[560px] max-sm:-mx-5 max-sm:rounded-none max-sm:p-5">
        <Image
          alt=""
          className="object-cover object-[50%_70%]"
          fill
          priority
          quality={90}
          sizes="(min-width: 1280px) 660px, (min-width: 768px) calc(100vw - 96px), (min-width: 640px) calc(100vw - 40px), 100vw"
          src="/paint/baml-dusk-stars.jpg"
        />
        <div className="relative h-full rise" style={delay(160)}>
          <Editor />
        </div>
      </div>
    </section>
  );
}
