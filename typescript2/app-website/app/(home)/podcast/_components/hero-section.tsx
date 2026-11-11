import { section } from '@/components/home/analytics';
import { PAGE_WRAP } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { NextLive } from './next-live';

const HOST_LINK =
  'inline-flex items-baseline gap-1.5 text-head underline decoration-accent decoration-1 underline-offset-[3px] hover:text-clover transition-colors';

// The hosts, each with their X photo, linking to their X profile.
const HOSTS = [
  { handle: 'vaibcode', photo: '/profile-vbv.jpeg' },
  { handle: 'dexhorthy', photo: '/host-dexhorthy.jpg' },
];

// The show streams on Vaibhav's Twitch; the recordings go to YouTube; the calendar has every session.
export const TWITCH = 'https://www.twitch.tv/vaibcode';
export const CALENDAR = 'https://lu.ma/baml';

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

/** The podcast page's header: the show, its hosts, when it's on, and the numbers behind it. */
export function HeroSection({
  episodes,
  since,
  views,
}: {
  episodes: number;
  since: string;
  views: string;
}) {
  const stats = [
    [views, 'views on YouTube'],
    [String(episodes), 'episodes'],
    ['Mostly weekly', `since ${since}`],
  ];
  return (
    <section
      {...section('podcast_header')}
      className={`${PAGE_WRAP} pt-14 md:pt-20 pb-12 md:pb-16`}
    >
      <h1 className="v4-h1">
        ai that <span className="text-clover">works</span>.
      </h1>
      <p className="mt-6 md:mt-8 max-w-[38rem] v4-body text-ink-2">
        A weekly conversation about getting more out of today&apos;s models with{' '}
        <Host {...HOSTS[0]} /> and <Host {...HOSTS[1]} />. Live code, Q&amp;A,
        and production AI patterns.
      </p>
      <p className="mt-6 text-[15px] text-body">
        <NextLive />, live on{' '}
        <a
          className="text-head underline decoration-accent decoration-1 underline-offset-[3px] transition-colors hover:text-clover"
          href={TWITCH}
          rel="noopener"
          target="_blank"
        >
          Twitch
        </a>
        .
      </p>
      <div className="mt-8 flex flex-wrap gap-2.5">
        <Button href={TWITCH}>Watch live on Twitch</Button>
        <Button href={LINKS.youtube} variant="secondary">
          Subscribe on YouTube
        </Button>
        <Button href={CALENDAR} variant="secondary">
          Add to your calendar
        </Button>
      </div>
      <dl className="mt-12 flex max-w-[44rem] flex-wrap gap-x-12 gap-y-6 border-t border-rule pt-6">
        {stats.map(([value, label]) => (
          <div className="flex flex-col-reverse justify-end gap-1" key={label}>
            <dt className="text-[13px] text-muted">{label}</dt>
            <dd className="font-display text-[28px] font-medium leading-none tracking-[-0.02em] text-head tabular-nums md:text-[34px]">
              {value}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
