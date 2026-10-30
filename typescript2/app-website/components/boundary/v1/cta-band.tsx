import { cta } from '../analytics';
import { BOOK_MEETING, CONTAINER } from '../page-parts';

/** The closing call to action of the frozen /v1 homepage. */
export function CtaBand() {
  return (
    <section className={`${CONTAINER} pt-6 md:pt-10 pb-20 md:pb-30`}>
      <div className="flex flex-col lg:flex-row lg:justify-between lg:items-center gap-8 lg:gap-16 p-8 md:p-14 rounded-[14px] bg-panel">
        <div className="flex flex-col gap-3 lg:w-180">
          <h2 className="h2-display !text-[clamp(28px,2.6vw,38px)]">
            Let your agents build faster.
          </h2>
          <p className="text-[17px] leading-[27px] text-ink-2">
            The AI observability platform for shipping faster to production.
          </p>
        </div>
        <a
          href={BOOK_MEETING}
          {...cta('book_meeting', 'cta_band')}
          className="focus-ring self-start lg:self-center py-3.25 px-5 whitespace-nowrap rounded-md bg-primary text-white text-[15px] leading-4.5 font-medium hover:bg-primary-hover active:translate-y-px transition"
        >
          Book a meeting
        </a>
      </div>
    </section>
  );
}
