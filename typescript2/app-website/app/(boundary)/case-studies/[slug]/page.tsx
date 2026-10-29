/** biome-ignore-all lint/suspicious/noArrayIndexKey: a study's blocks are static content, never reordered */
import type { Metadata } from 'next';
import Image from 'next/image';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import Footer from '@/components/boundary/footer';
import Nav from '@/components/boundary/nav';
import {
  BackLink,
  CONTAINER,
  CtaBand,
  OG_SITE,
} from '@/components/boundary/page-parts';
import PageTransition from '@/components/boundary/page-transition';
import Prose from '@/components/boundary/prose';
import {
  getAllCaseStudies,
  getCaseStudy,
  getNextCaseStudy,
} from '@/content/boundary/case-studies';
import type { CaseStudy } from '@/content/boundary/types';

export const dynamicParams = false;

/** The rail's contents: the study's own sections, then Results and the quote when the study has them. */
function sectionIndex(study: CaseStudy): { id: string; label: string }[] {
  return [
    ...study.sections.map((s) => ({ id: s.id, label: s.heading })),
    ...(study.results?.length ? [{ id: 'results', label: 'Results' }] : []),
    ...(study.quote
      ? [{ id: 'in-their-words', label: 'In their own words' }]
      : []),
  ];
}

export function generateStaticParams() {
  return getAllCaseStudies().map((c) => ({ slug: c.slug }));
}

export async function generateMetadata(
  props: PageProps<'/case-studies/[slug]'>,
): Promise<Metadata> {
  const { slug } = await props.params;
  const study = getCaseStudy(slug);
  if (!study) return {};
  return {
    description: study.dek,
    openGraph: {
      ...OG_SITE,
      description: study.dek,
      publishedTime: study.date,
      tags: study.tags,
      title: `${study.company} case study · Boundary`,
      type: 'article',
    },
    title: `${study.company} case study`,
  };
}

export default async function CaseStudyPage(
  props: PageProps<'/case-studies/[slug]'>,
) {
  const { slug } = await props.params;
  const study = getCaseStudy(slug);
  if (!study) notFound();
  const next = getNextCaseStudy(slug);

  return (
    <>
      <Nav />
      <PageTransition id={`case-studies/${slug}`}>
        <main className="flex-1">
          <div className={CONTAINER}>
            <BackLink href="/case-studies" label="Case studies" />
          </div>

          <article>
            {/* The homepage case-study block, opened out: painting and company on the left, the
              headline and the facts on the right. Same panel, same radius, same right column. */}
            <header className={`${CONTAINER} pt-6 md:pt-8 pb-10 md:pb-16`}>
              <div className="flex flex-col xl:flex-row xl:min-h-110 rounded-[14px] overflow-hidden bg-panel">
                <div className="relative xl:w-[44%] shrink-0 h-56 xl:h-auto flex items-end p-8 bg-pasture">
                  <Image
                    alt=""
                    className="object-cover"
                    fill
                    priority
                    quality={90}
                    sizes="(min-width: 1280px) 600px, 100vw"
                    src={study.painting}
                    style={{ objectPosition: study.crop }}
                  />
                  <span className="relative font-display font-medium text-[32px] md:text-[40px] leading-[1.08] tracking-[-0.03em] text-white">
                    {study.company}
                  </span>
                </div>

                <div className="flex-1 min-w-0 flex flex-col justify-between gap-9 p-8 md:px-14 md:py-12">
                  <div className="flex flex-col gap-5">
                    <h1 className="font-display font-medium text-[clamp(26px,2.6vw,36px)] leading-[1.2] tracking-[-0.03em] text-ink text-balance">
                      {study.title}
                    </h1>
                    <p className="text-[17px] leading-[27px] text-ink-2">
                      {study.dek}
                    </p>
                  </div>

                  {/* two columns on a phone, four from sm: the first cell of each row loses its rule */}
                  <dl className="grid grid-cols-2 sm:grid-cols-4 border-t border-card-line">
                    {study.facts.map((f, i) => (
                      <div
                        className={`flex flex-col gap-1.5 pt-4 pb-1 pr-4 pl-4 border-l border-card-line ${
                          i === 0
                            ? 'pl-0 border-l-0'
                            : i === 2
                              ? 'max-sm:pl-0 max-sm:border-l-0'
                              : ''
                        }`}
                        key={f.term}
                      >
                        <dt className="text-sm leading-5 text-ink-2">
                          {f.term}
                        </dt>
                        <dd className="text-[15px] leading-5 text-ink">
                          {f.value}
                        </dd>
                      </div>
                    ))}
                  </dl>
                </div>
              </div>
            </header>

            {/* Loop's grid: a sticky index on the left, the body on the right at a reading measure.
              The rail and the hero card above it start at the same x, so the page holds one edge. */}
            <div
              className={`${CONTAINER} flex flex-col lg:flex-row gap-10 lg:gap-16`}
            >
              <nav aria-label="On this page" className="lg:w-105 shrink-0">
                <div className="lg:sticky lg:top-26 flex flex-col gap-4">
                  <span className="text-sm leading-5 text-ink-2">
                    On this page
                  </span>
                  <ol className="flex flex-col border-t border-ink">
                    {sectionIndex(study).map(({ id, label }) => (
                      <li className="border-b border-hair" key={id}>
                        <a
                          className="focus-ring block py-3 text-[15px] leading-6 text-ink-2 hover:text-ink transition-colors"
                          href={`#${id}`}
                        >
                          {label}
                        </a>
                      </li>
                    ))}
                  </ol>
                </div>
              </nav>

              <div className="flex-1 min-w-0 max-w-[68ch]">
                {study.sections.map((s) => (
                  <section className="pt-10 md:pt-12 first:pt-0" key={s.id}>
                    <h2
                      className="font-display font-medium text-[24px] md:text-[28px] leading-[1.22] tracking-[-0.03em] text-ink"
                      id={s.id}
                    >
                      {s.heading}
                    </h2>
                    <Prose blocks={s.body} />
                  </section>
                ))}

                {study.results?.length ? (
                  <section className="pt-12 md:pt-16">
                    <h2
                      className="mb-6 font-display font-medium text-[24px] md:text-[28px] leading-[1.22] tracking-[-0.03em] text-ink"
                      id="results"
                    >
                      Results
                    </h2>
                    <div className="grid grid-cols-1 sm:grid-cols-3 border-t border-ink">
                      {study.results.map((r, i) => (
                        <div
                          className={`flex flex-col gap-1.5 pt-6 pb-5 pr-6 ${
                            i
                              ? 'border-t sm:border-t-0 sm:border-l sm:pl-6 border-hair'
                              : ''
                          }`}
                          key={r.label + i}
                        >
                          <span className="font-display text-[clamp(38px,3.6vw,52px)] leading-[1.06] tracking-[-0.04em] text-accent">
                            {r.value}
                          </span>
                          <span className="text-[15px] leading-[22px] text-ink-2">
                            {r.label}
                          </span>
                        </div>
                      ))}
                    </div>
                  </section>
                ) : null}

                {study.quote && (
                  <figure
                    className="flex flex-col gap-4 pt-12 md:pt-16"
                    id="in-their-words"
                  >
                    <blockquote className="font-display text-[clamp(22px,2.3vw,30px)] leading-[1.28] tracking-[-0.03em] text-ink text-balance">
                      “{study.quote.text}”
                    </blockquote>
                    <figcaption className="text-[15px] leading-6 text-ink-2">
                      <span className="font-medium text-ink">
                        {study.quote.name}
                      </span>
                      , {study.quote.role}, {study.quote.company}
                    </figcaption>
                  </figure>
                )}

                {next && (
                  <footer className="flex flex-col gap-3 mt-12 md:mt-16 mb-14 md:mb-20 pt-7 border-t border-ink">
                    <span className="text-sm leading-5 text-ink-2">
                      Next case study
                    </span>
                    <Link
                      className="focus-ring group flex flex-col gap-1"
                      href={`/case-studies/${next.slug}`}
                    >
                      <span className="font-display font-medium text-[20px] leading-7 tracking-[-0.03em] text-ink group-hover:text-accent transition-colors">
                        {next.company}
                      </span>
                      <span className="text-[15px] leading-6 text-ink-2">
                        {next.title}
                      </span>
                    </Link>
                  </footer>
                )}
              </div>
            </div>
          </article>

          <CtaBand />
        </main>
      </PageTransition>
      <Footer />
    </>
  );
}
