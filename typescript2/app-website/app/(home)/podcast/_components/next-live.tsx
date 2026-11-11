'use client';

import { useEffect, useState } from 'react';

// When the show is on, in the reader's own time zone. It streams Tuesdays at 10:15 AM Pacific for
// about an hour; the server renders Pacific time, and the browser swaps in its own zone (and day,
// for readers where that's already Wednesday).

const ZONE = 'America/Los_Angeles';
const DAY = 2; // Tuesday
const HOUR = 10;
const MINUTE = 15;
const LENGTH_MS = 75 * 60 * 1000;

/** The wall-clock parts of an instant in a time zone. */
function partsIn(at: Date, zone: string) {
  const parts = new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    hour: 'numeric',
    hourCycle: 'h23',
    minute: 'numeric',
    month: 'numeric',
    timeZone: zone,
    weekday: 'short',
    year: 'numeric',
  }).formatToParts(at);
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? '';
  const weekday = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'].indexOf(
    get('weekday'),
  );
  return {
    day: Number(get('day')),
    hour: Number(get('hour')),
    minute: Number(get('minute')),
    month: Number(get('month')),
    weekday,
    year: Number(get('year')),
  };
}

/** The instant a Pacific wall-clock time happens. */
function pacific(year: number, month: number, day: number): Date {
  const guess = Date.UTC(year, month - 1, day, HOUR, MINUTE);
  // Pacific is UTC-7 or -8: the offset is whatever makes the wall clock read right.
  for (const offset of [7, 8]) {
    const at = new Date(guess + offset * 3_600_000);
    const p = partsIn(at, ZONE);
    if (p.hour === HOUR && p.minute === MINUTE) return at;
  }
  return new Date(guess + 8 * 3_600_000);
}

/** The start of this week's show if it's on now, otherwise the next one. */
function nextShow(now: Date): Date {
  const p = partsIn(now, ZONE);
  const ahead = (DAY - p.weekday + 7) % 7;
  const base = new Date(Date.UTC(p.year, p.month - 1, p.day + ahead));
  let start = pacific(
    base.getUTCFullYear(),
    base.getUTCMonth() + 1,
    base.getUTCDate(),
  );
  if (now.getTime() > start.getTime() + LENGTH_MS) {
    const next = new Date(base.getTime() + 7 * 86_400_000);
    start = pacific(
      next.getUTCFullYear(),
      next.getUTCMonth() + 1,
      next.getUTCDate(),
    );
  }
  return start;
}

function describe(start: Date, zone: string | undefined) {
  const weekday = new Intl.DateTimeFormat('en-US', {
    timeZone: zone,
    weekday: 'long',
  }).format(start);
  const time = new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    timeZone: zone,
    timeZoneName: 'short',
  }).format(start);
  return { time, weekday };
}

/** "Every Tuesday at 10:15 AM PST", in the reader's zone once the page loads, or "Live now". */
export function NextLive() {
  const [now, setNow] = useState<Date | null>(null);
  useEffect(() => {
    setNow(new Date());
    const t = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(t);
  }, []);

  const start = nextShow(now ?? new Date());
  const live =
    now !== null &&
    now.getTime() >= start.getTime() &&
    now.getTime() < start.getTime() + LENGTH_MS;
  // Before the browser knows its zone, say it in Pacific, so the server and client agree.
  const { time, weekday } = describe(start, now ? undefined : ZONE);

  if (live)
    return (
      <span className="inline-flex items-center gap-2">
        <span className="relative flex size-2">
          <span className="absolute inline-flex size-full animate-ping rounded-full bg-[#ff4d4d] opacity-75" />
          <span className="relative inline-flex size-2 rounded-full bg-[#ff4d4d]" />
        </span>
        <strong className="font-medium text-head">Live now</strong>
      </span>
    );
  return (
    <span suppressHydrationWarning>
      Every <strong className="font-medium text-head">{weekday}</strong> at{' '}
      <strong className="font-medium text-head">{time}</strong>
    </span>
  );
}
