import { Calendar } from 'lucide-react';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP } from '@/components/home/page-shell';

const HOST_LINK =
  'text-head underline decoration-accent decoration-1 underline-offset-[3px] hover:text-clover transition-colors';

/** The podcast page's header: the show's name, its hosts, and when it airs. */
export function HeroSection() {
  return (
    <section
      {...section('podcast_header')}
      className={`${PAGE_WRAP} pt-14 md:pt-20 pb-10 md:pb-14`}
    >
      <p className="v4-caption text-muted">Weekly sessions</p>
      <h1 className="mt-5 v4-h1">
        ai that <span className="text-clover">works</span>.
      </h1>
      <p className="mt-6 md:mt-8 max-w-[38rem] v4-body text-ink-2">
        A weekly conversation about getting more out of today&apos;s models with{' '}
        <a className={HOST_LINK} href="https://www.github.com/hellovai">
          @hellovai
        </a>{' '}
        and{' '}
        <a className={HOST_LINK} href="https://www.github.com/dexhorthy">
          @dexhorthy
        </a>
        . Live code, Q&amp;A, and production AI patterns.
      </p>
      <p className="mt-7 flex items-center gap-2 text-[15px] text-body">
        <Calendar aria-hidden className="size-4 text-muted" />
        <span>
          Every <strong className="font-medium text-head">Tuesday</strong> at{' '}
          <strong className="font-medium text-head">10 AM PST</strong>
        </span>
      </p>
    </section>
  );
}
