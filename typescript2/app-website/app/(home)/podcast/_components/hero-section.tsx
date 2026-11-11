import { Calendar } from 'lucide-react';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP } from '@/components/home/page-shell';

const HOST_LINK =
  'inline-flex items-baseline gap-1.5 text-head underline decoration-accent decoration-1 underline-offset-[3px] hover:text-clover transition-colors';

// The hosts, each with their X photo, linking to their X profile.
const HOSTS = [
  { handle: 'vaibcode', photo: '/host-vaibcode.jpg' },
  { handle: 'dexhorthy', photo: '/host-dexhorthy.jpg' },
];

// The show streams on Vaibhav's Twitch.
const TWITCH = 'https://www.twitch.tv/vaibcode';

function Host({ handle, photo }: (typeof HOSTS)[number]) {
  return (
    <a
      className={HOST_LINK}
      href={`https://x.com/${handle}`}
      rel="noopener"
      target="_blank"
    >
      {/* biome-ignore lint/performance/noImgElement: a small static photo */}
      <img
        alt=""
        className="size-6 self-center rounded-full object-cover"
        height={24}
        src={photo}
        width={24}
      />
      @{handle}
    </a>
  );
}

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
        <Host {...HOSTS[0]} /> and <Host {...HOSTS[1]} />. Live code, Q&amp;A,
        and production AI patterns.
      </p>
      <a
        className="group mt-7 inline-flex items-center gap-2 text-[15px] text-body transition-colors hover:text-head"
        href={TWITCH}
        rel="noopener"
        target="_blank"
      >
        <Calendar aria-hidden className="size-4 text-muted" />
        <span>
          Every <strong className="font-medium text-head">Tuesday</strong> at{' '}
          <strong className="font-medium text-head">10:15 AM PST</strong>, live on{' '}
          <span className="text-head underline decoration-accent decoration-1 underline-offset-[3px] group-hover:text-clover">
            Twitch
          </span>
        </span>
      </a>
    </section>
  );
}
