'use client';

import { ArrowRight } from 'lucide-react';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import type { EapEvent } from '@/lib/luma';
import { dateLabel, timeLabel } from '@/lib/session-time';
import { sessionTiming } from '@/lib/signup';
import { SIGNUP } from './links';

type BannerSession = Pick<EapEvent, 'id' | 'start_at' | 'end_at' | 'timezone'>;

export function EarlyAccessBanner({ sessions }: { sessions: BannerSession[] }) {
  const [clock, setClock] = useState<{ now: Date; zone: string }>();
  useEffect(() => {
    const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const update = () => setClock({ now: new Date(), zone });
    update();
    const interval = window.setInterval(update, 30_000);
    window.addEventListener('focus', update);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener('focus', update);
    };
  }, []);

  const session = sessions.find(
    ({ end_at }) => !clock || Date.parse(end_at) > clock.now.getTime(),
  );
  const start = session ? new Date(session.start_at) : undefined;
  const zone = clock?.zone ?? session?.timezone;
  const timing =
    clock && session && sessionTiming(session, clock.now.getTime());
  const status =
    timing === 'live' ? 'Live now' : timing === 'soon' ? 'Starts soon' : null;

  return (
    <Link
      className="group flex min-h-9 flex-wrap items-center justify-center gap-x-2.5 px-3 py-1.5 text-center text-xs leading-tight text-on-accent bg-accent transition-colors hover:bg-accent-hover focus-visible:outline-2 focus-visible:-outline-offset-4 focus-visible:outline-on-accent sm:text-[13px]"
      href={
        session ? `${SIGNUP}?session=${encodeURIComponent(session.id)}` : SIGNUP
      }
    >
      <span className="font-semibold underline-offset-2 group-hover:underline">
        Get early access
      </span>
      {session && start && zone && (
        <time
          className="whitespace-nowrap opacity-85"
          dateTime={session.start_at}
        >
          {status ?? `${dateLabel(start, zone)} · ${timeLabel(start, zone)}`}
        </time>
      )}
      <ArrowRight aria-hidden="true" size={13} />
    </Link>
  );
}
