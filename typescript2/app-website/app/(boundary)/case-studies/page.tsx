import type { Metadata } from 'next';
import Image from 'next/image';
import Link from 'next/link';
import Footer from '@/components/boundary/footer';
import Nav from '@/components/boundary/nav';
import { CONTAINER, CtaBand, OG_SITE } from '@/components/boundary/page-parts';
import PageTransition from '@/components/boundary/page-transition';
import { getAllCaseStudies } from '@/content/boundary/case-studies';

const DESCRIPTION =
  'How teams write their AI code in BAML and run it on Boundary.';

export const metadata: Metadata = {
  description: DESCRIPTION,
  openGraph: {
    ...OG_SITE,
    description: DESCRIPTION,
    title: 'Case studies · Boundary',
    type: 'website',
  },
  title: 'Case studies',
};

export default function CaseStudiesIndex() {
  const studies = getAllCaseStudies();

  return (
    <>
      <Nav />
      <PageTransition id={'case-studies'}>
        <main className="flex-1">
          {/* Heading left, one line of context right: the same header the Stack section uses. */}
          <section className={`${CONTAINER} pt-14 md:pt-22 pb-10 md:pb-14`}>
            <header className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-5 md:gap-8">
              <h1 className="max-w-205 h2-display">Case studies</h1>
              <p className="lg:w-100 shrink-0 text-[17px] leading-[27px] text-ink-2">
                Teams who write their AI code in BAML can query what every call
                did, without adding a line of logging.
              </p>
            </header>
          </section>

          <section className={`${CONTAINER} pb-12 md:pb-18`}>
            <ul className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
              {studies.map((c) => (
                <li className="flex" key={c.slug}>
                  <Link
                    className="focus-ring group flex flex-col w-full rounded-[14px] overflow-hidden bg-panel"
                    href={`/case-studies/${c.slug}`}
                  >
                    {/* The painting panel from the homepage case-study block, at card size. The panel
                      clips on its own because the painting is scaled up inside it (see cardZoom). */}
                    <div className="relative h-44 overflow-hidden flex items-end p-6 bg-pasture">
                      <Image
                        alt=""
                        className="object-cover"
                        fill
                        quality={90}
                        sizes="(min-width: 1024px) 440px, (min-width: 768px) 50vw, 100vw"
                        src={c.painting}
                        style={{
                          objectPosition: c.crop,
                          scale: c.cardZoom,
                          transformOrigin: c.crop,
                        }}
                      />
                      <span className="relative font-display font-medium text-[26px] leading-7 tracking-[-0.03em] text-white">
                        {c.company}
                      </span>
                    </div>

                    <div className="flex-1 flex flex-col gap-4 p-6 md:p-7">
                      <h2 className="font-display font-medium text-[20px] leading-[1.28] tracking-[-0.03em] text-ink group-hover:text-accent transition-colors">
                        {c.title}
                      </h2>
                      <p className="text-[15px] leading-6 text-ink-2">
                        {c.dek}
                      </p>
                      <span className="mt-auto flex flex-col gap-4 pt-2">
                        <span className="self-start text-[15px] leading-5 font-medium text-ink ulink group-hover:text-accent transition-colors">
                          Read the case study
                        </span>
                      </span>
                    </div>
                  </Link>
                </li>
              ))}
            </ul>
          </section>

          <CtaBand />
        </main>
      </PageTransition>
      <Footer />
    </>
  );
}
