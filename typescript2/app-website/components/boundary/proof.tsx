import Image from 'next/image';
import Link from 'next/link';
import { getFeaturedCaseStudy } from '@/content/boundary/case-studies';
import { ParallaxImage, Resolve, Rise } from './motion';

const STATS = [
  ['6 → 1', 'tools, accounts and API keys, down to one install'],
  ['0', 'SDKs to wire up, because tracing is built in'],
  ['7', 'languages from one BAML codebase'],
  ['10+', 'model providers, plus any compatible API'],
];

/** Stats on a band of the painting. */
export function Metrics() {
  return (
    <section className="relative overflow-hidden bg-pasture flex flex-col justify-end pt-20 md:pt-28">
      <ParallaxImage
        overscan={15}
        quality={90}
        sizes="100vw"
        src="/boundary/painting-footer-4k.jpg"
        strength={0.1}
      />
      <div className="absolute inset-x-0 bottom-0 h-80 bg-gradient-to-b from-[#0E2306]/0 to-[#0E2306]/55" />
      <div className="relative w-full max-w-[1440px] mx-auto px-5 md:px-12 pb-16">
        <div className="grid grid-cols-2 lg:grid-cols-4 border-t border-white/35">
          {STATS.map(([n, label], i) => (
            <div
              className={`flex flex-col gap-2 pt-6 ${i ? 'lg:pl-6 lg:border-l border-white/20' : ''} pr-6`}
              key={label}
            >
              <Resolve
                className="font-display text-[clamp(44px,4.4vw,64px)] leading-[1.06] tracking-[-0.04em] text-white"
                value={n}
              />
              <span className="text-[15px] leading-[22px] text-white/80">
                {label}
              </span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

export function Testimonials() {
  return (
    <section className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-18 md:pt-34 pb-16 md:pb-30">
      <div className="flex flex-col lg:flex-row gap-14 lg:gap-16 items-stretch">
        <figure className="lg:w-195 shrink-0 flex flex-col justify-between gap-12">
          <Rise
            as="blockquote"
            className="font-display text-[clamp(24px,2.7vw,38px)] leading-[1.26] tracking-[-0.03em] text-ink"
            text="“Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua, ut enim ad minim.”"
          />
          <figcaption className="flex items-center gap-3.5">
            <span className="size-11 rounded-full bg-card-line" />
            <span className="flex flex-col gap-0.5">
              <span className="text-[15px] leading-5 font-medium text-ink">
                Firstname Lastname
              </span>
              <span className="text-sm leading-5 text-ink-2">
                Head of Engineering, Ledgerline
              </span>
            </span>
          </figcaption>
        </figure>
        <div className="flex-1 flex flex-col border-t border-ink">
          {[
            [
              '“Lorem ipsum dolor sit amet, consectetur adipiscing elit. Duis aute irure dolor in reprehenderit.”',
              'Firstname Lastname · Staff Engineer, Kestrel Labs',
            ],
            [
              '“Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim.”',
              'Firstname Lastname · CTO, Sundial',
            ],
          ].map(([q, who], i) => (
            <figure
              className={`flex flex-col gap-5 py-7 ${i === 0 ? 'border-b border-hair' : ''}`}
              key={who}
            >
              <blockquote className="text-[17px] leading-[27px] text-ink">
                {q}
              </blockquote>
              <figcaption className="text-sm leading-5 text-ink-2">
                {who}
              </figcaption>
            </figure>
          ))}
        </div>
      </div>
    </section>
  );
}

// The card points at the newest case study in content/case-studies.ts. The company name here stays
// hard-coded so the homepage keeps the wording Boundary signed off on.
export function CaseStudy() {
  const featured = getFeaturedCaseStudy();
  return (
    <section className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pb-20 md:pb-34">
      <div className="flex flex-col lg:flex-row lg:h-130 rounded-[14px] overflow-hidden bg-panel">
        <div className="relative lg:w-160 shrink-0 h-72 lg:h-auto flex items-end p-8 bg-pasture">
          <Image
            alt=""
            className="object-cover object-[30%_60%]"
            fill
            quality={90}
            sizes="(min-width: 1024px) 640px, 100vw"
            src="/boundary/painting-hills.jpg"
          />
          <span className="relative font-display font-medium text-[28px] leading-8 tracking-[-0.03em] text-white">
            Ledgerline
          </span>
        </div>
        <div className="flex-1 flex flex-col justify-between gap-10 p-8 md:px-14 md:py-12">
          <Rise
            as="h3"
            className="font-display font-medium text-[clamp(24px,2.6vw,36px)] leading-[1.2] tracking-[-0.03em] text-ink"
            text="Lorem ipsum dolor sit amet, consectetur adipiscing elit sed do."
          />
          <div className="flex flex-col gap-7">
            <div className="flex gap-12">
              {[
                ['00%', 'lorem ipsum dolor'],
                ['0 days', 'lorem ipsum sit amet'],
              ].map(([n, l]) => (
                <div className="flex flex-col gap-1" key={l}>
                  <Resolve
                    className="font-display text-[40px] leading-11 tracking-[-0.04em] text-accent"
                    value={n}
                  />
                  <span className="text-sm leading-5 text-ink-2">{l}</span>
                </div>
              ))}
            </div>
            <Link
              className="focus-ring self-start text-[15px] leading-5 font-medium text-ink ulink hover:text-accent"
              href={`/case-studies/${featured.slug}`}
            >
              Read the case study
            </Link>
          </div>
        </div>
      </div>
    </section>
  );
}
