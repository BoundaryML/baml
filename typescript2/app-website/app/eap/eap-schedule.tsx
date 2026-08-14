'use client';

import { type ReactNode, useEffect, useState } from 'react';
import type { EapEvent } from '@/lib/luma';
import { RegisterModal } from './register-modal';

const MIN = 60_000;
const LUMA_CALENDAR_URL = 'https://luma.com/baml';

// --- formatting helpers. Pass timeZone === undefined to render in the viewer's
// local zone; pass the event's zone to render in Pacific. ---

function dateStr(iso: string, timeZone?: string): string {
  return new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'short',
    timeZone,
    weekday: 'short',
  }).format(new Date(iso));
}
function timeStr(iso: string, timeZone?: string, withZone = false): string {
  return new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    timeZone,
    timeZoneName: withZone ? 'short' : undefined,
  }).format(new Date(iso));
}
function weekdayPlural(iso: string, timeZone?: string): string {
  const wd = new Intl.DateTimeFormat('en-US', {
    timeZone,
    weekday: 'long',
  }).format(new Date(iso));
  return `${wd}s`;
}
function ptClock(event: EapEvent): string {
  // The Pacific time-of-day, e.g. "10:00 AM", used as the slot key and label.
  return timeStr(event.start_at, event.timezone);
}

function relative(startMs: number, now: number): string {
  const mins = Math.round((startMs - now) / MIN);
  if (mins < 60) return mins <= 1 ? 'starting soon' : `in ${mins} min`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `in ${hrs} hour${hrs === 1 ? '' : 's'}`;
  const days = Math.round(hrs / 24);
  return `in ${days} day${days === 1 ? '' : 's'}`;
}
function endsIn(endMs: number, now: number): string {
  const mins = Math.max(0, Math.round((endMs - now) / MIN));
  return mins < 1 ? 'ending now' : `ends in ${mins} min`;
}
function goingLabel(n: number | null): string | null {
  return n && n > 0 ? `${n} going` : null;
}

// The hour (0-23) the event lands at in the viewer's local zone.
function localHour(iso: string): number {
  return Number(
    new Intl.DateTimeFormat('en-US', {
      hour: '2-digit',
      hourCycle: 'h23',
    }).format(new Date(iso)),
  );
}

// How convenient a local hour is, higher = better. Comfortable window is roughly
// 8am-9pm, peaking early afternoon; hours outside are penalised the deeper into
// the night they fall. Lets us surface the slot that lands at a sane local time.
function convenience(hour: number): number {
  if (hour >= 8 && hour <= 21) return 100 - Math.abs(hour - 13);
  const intoNight = hour < 8 ? 8 - hour : hour - 21;
  return -Math.min(intoNight, 12) * 12;
}

interface Avail {
  // Only set when the session is NOT plainly open, so open sessions carry no
  // badge (most are open, so a badge on every one is just noise).
  badge: { cls: string; label: string } | null;
  bookable: boolean;
}
function availabilityOf(event: EapEvent): Avail {
  // Luma exposes no capacity, so registration_open is the reliable signal: when
  // the host closes it (full or past cutoff) new sign-ups go to the waitlist.
  if (event.registration_open === false) {
    return event.waitlist_status === 'enabled'
      ? { badge: { cls: 'wait', label: 'Waitlist' }, bookable: true }
      : { badge: { cls: 'full', label: 'Full' }, bookable: false };
  }
  return { badge: null, bookable: true };
}

function CalTile({
  event,
  local,
  variant,
  minimal,
}: {
  event: EapEvent;
  local: boolean;
  variant?: 'accent' | 'live';
  minimal?: boolean;
}) {
  const tz = local ? undefined : event.timezone;
  const month = new Intl.DateTimeFormat('en-US', {
    month: 'short',
    timeZone: tz,
  }).format(new Date(event.start_at));
  const day = new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    timeZone: tz,
  }).format(new Date(event.start_at));

  // Rows get a flat, minimal date instead of the boxed tile the featured card
  // uses, so the list reads lighter.
  if (minimal) {
    return (
      <div className="eap-date">
        <span className="eap-date-m">{month}</span>
        <span className="eap-date-d">{day}</span>
      </div>
    );
  }

  return (
    <div className={`eap-cal${variant ? ` eap-cal--${variant}` : ''}`}>
      <div className="eap-cal-m">{month}</div>
      <div className="eap-cal-d">{day}</div>
    </div>
  );
}

function FeaturedNext({
  event,
  now,
  local,
  onBook,
}: {
  event: EapEvent;
  now: number | null;
  local: boolean;
  onBook: (event: EapEvent) => void;
}) {
  const going = goingLabel(event.goingCount);
  const availability = availabilityOf(event);
  return (
    <a
      className="eap-card eap-featured"
      href={event.url}
      onClick={(e) => {
        if (availability.bookable) {
          e.preventDefault();
          onBook(event);
        }
      }}
      rel="noopener noreferrer"
      target="_blank"
    >
      <CalTile event={event} local={local} variant="accent" />
      <div className="eap-mid">
        <div className="eap-flag">
          <span className="eap-flag-lbl">Next session</span>
          {now != null && (
            <span className="eap-flag-soon">
              {relative(new Date(event.start_at).getTime(), now)}
            </span>
          )}
        </div>
        {local ? (
          <>
            <div className="eap-when">
              {dateStr(event.start_at)} · {timeStr(event.start_at)}
            </div>
            <div className="eap-tz">{ptClock(event)} PT</div>
          </>
        ) : (
          <div className="eap-when">
            {dateStr(event.start_at, event.timezone)} ·{' '}
            {timeStr(event.start_at, event.timezone, true)}
          </div>
        )}
        {(availability.badge || going) && (
          <div className="eap-meta">
            {availability.badge && (
              <span className={`eap-badge ${availability.badge.cls}`}>
                {availability.badge.label}
              </span>
            )}
            {going && <span className="eap-going">{going}</span>}
          </div>
        )}
      </div>
      <span className="eap-book">
        {availability.bookable ? 'Book this session' : 'View session'}{' '}
        <span className="eap-arw">→</span>
      </span>
    </a>
  );
}

function FeaturedLive({ event, now }: { event: EapEvent; now: number }) {
  const going = goingLabel(event.goingCount);
  return (
    <a
      className="eap-card eap-featured eap-featured--live"
      href={event.url}
      rel="noopener noreferrer"
      target="_blank"
    >
      <CalTile event={event} local variant="live" />
      <div className="eap-mid">
        <div className="eap-flag">
          <span className="eap-badge live">Live now</span>
          <span className="eap-flag-soon">
            {endsIn(new Date(event.end_at).getTime(), now)}
          </span>
        </div>
        <div className="eap-when">In progress</div>
        <div className="eap-tz">
          Started {timeStr(event.start_at)} · ends {timeStr(event.end_at)}
        </div>
        {going && (
          <div className="eap-meta">
            <span className="eap-going">{going}</span>
          </div>
        )}
      </div>
      <span className="eap-book">
        Join now <span className="eap-arw">→</span>
      </span>
    </a>
  );
}

function Row({
  event,
  local,
  onBook,
}: {
  event: EapEvent;
  local: boolean;
  onBook: (event: EapEvent) => void;
}) {
  const availability = availabilityOf(event);
  const going = goingLabel(event.goingCount);
  const tz = local ? undefined : event.timezone;
  const meta = availability.badge || going;
  // The date already reads off the calendar tile and the group header supplies
  // the weekday + time, so the row itself only needs the tile, an exception badge
  // (Full / Waitlist), and the CTA. Full date stays on the link for screen readers.
  return (
    <a
      aria-label={`${dateStr(event.start_at, tz)}, ${availability.badge?.label ?? 'open'}`}
      className="eap-card"
      href={event.url}
      onClick={(e) => {
        if (availability.bookable) {
          e.preventDefault();
          onBook(event);
        }
      }}
      rel="noopener noreferrer"
      target="_blank"
    >
      <CalTile event={event} local={local} minimal />
      <div className="eap-mid">
        {meta && (
          <div className="eap-meta eap-meta--lead">
            {availability.badge && (
              <span className={`eap-badge ${availability.badge.cls}`}>
                {availability.badge.label}
              </span>
            )}
            {going && <span className="eap-going">{going}</span>}
          </div>
        )}
      </div>
      <span className="eap-book">
        {availability.bookable ? 'Book' : 'View'}{' '}
        <span className="eap-arw">→</span>
      </span>
    </a>
  );
}

function slotHint(event: EapEvent): string {
  const hour = Number(
    new Intl.DateTimeFormat('en-US', {
      hour: 'numeric',
      hour12: false,
      timeZone: event.timezone,
    }).format(new Date(event.start_at)),
  );
  return hour < 12
    ? 'Morning in the Americas, afternoon in Europe'
    : 'Evening in the Americas, morning in Asia-Pacific';
}

function GroupHeader({ sample, local }: { sample: EapEvent; local: boolean }) {
  const tz = local ? undefined : sample.timezone;
  return (
    <div className="eap-group-head">
      <div className="eap-gh-time">
        {weekdayPlural(sample.start_at, tz)} at{' '}
        {local ? (
          <>
            {timeStr(sample.start_at)}
            <span className="eap-gh-pt">{ptClock(sample)} PT</span>
          </>
        ) : (
          <>{ptClock(sample)} PT</>
        )}
      </div>
      <div className="eap-gh-hint">{slotHint(sample)}</div>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="eap-empty">
      No sessions are scheduled right now. Browse the{' '}
      <a href={LUMA_CALENDAR_URL} rel="noopener noreferrer" target="_blank">
        BAML calendar
      </a>{' '}
      for upcoming events.
    </div>
  );
}

function groupBySlot(
  events: EapEvent[],
): { events: EapEvent[]; key: string }[] {
  const map = new Map<string, EapEvent[]>();
  for (const event of events) {
    const key = ptClock(event);
    const list = map.get(key);
    if (list) {
      list.push(event);
    } else {
      map.set(key, [event]);
    }
  }
  return [...map.entries()].map(([key, evs]) => ({ events: evs, key }));
}

// Rank slots so the one that lands at a convenient LOCAL hour comes first. Only
// meaningful once we know the viewer's zone (local); before that we fall back to
// soonest-first so the server render stays deterministic.
function orderSlots(
  slots: { events: EapEvent[]; key: string }[],
  local: boolean,
): { events: EapEvent[]; key: string }[] {
  return [...slots].sort((a, b) => {
    if (local) {
      const diff =
        convenience(localHour(b.events[0].start_at)) -
        convenience(localHour(a.events[0].start_at));
      if (diff !== 0) return diff;
    }
    return (
      new Date(a.events[0].start_at).getTime() -
      new Date(b.events[0].start_at).getTime()
    );
  });
}

export function EapSchedule({
  events,
  discord,
}: {
  events: EapEvent[];
  discord: ReactNode;
}) {
  // now stays null until mount so the server render and first client render
  // agree (both show the soonest session in Pacific time). After mount we know
  // the real clock and the viewer's zone, so we can flag a live session, switch
  // every time to the viewer's local zone, and prefer the convenient slot.
  const [now, setNow] = useState<number | null>(null);
  const [openEvent, setOpenEvent] = useState<EapEvent | null>(null);
  useEffect(() => {
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), MIN);
    return () => clearInterval(id);
  }, []);

  const local = now != null;

  const sorted = [...events].sort(
    (a, b) => new Date(a.start_at).getTime() - new Date(b.start_at).getTime(),
  );

  let live: EapEvent | null = null;
  let upcoming = sorted;
  if (now != null) {
    live =
      sorted.find(
        (e) =>
          new Date(e.start_at).getTime() <= now &&
          now < new Date(e.end_at).getTime(),
      ) ?? null;
    upcoming = sorted.filter((e) => new Date(e.start_at).getTime() > now);
  }

  const slots = orderSlots(groupBySlot(upcoming), local);

  // Feature a live session if there is one; otherwise the soonest session in the
  // most convenient slot (which, before mount, is just the soonest overall).
  const featured = live ?? slots[0]?.events[0] ?? null;
  const groups = live
    ? slots
    : orderSlots(
        groupBySlot(upcoming.filter((e) => e !== featured)),
        local,
      ).filter((g) => g.events.length > 0);

  if (!featured && groups.length === 0) {
    return (
      <>
        <EmptyState />
        {discord}
      </>
    );
  }

  const featuredNode = live ? (
    <FeaturedLive event={live} now={now as number} />
  ) : (
    featured && (
      <FeaturedNext
        event={featured}
        local={local}
        now={now}
        onBook={setOpenEvent}
      />
    )
  );

  const otherTimes = groups.length > 0 && (
    <div className="eap-groups">
      {groups.map((group) => (
        <section className="eap-group" key={group.key}>
          <GroupHeader local={local} sample={group.events[0]} />
          <div className="eap-rows">
            {group.events.map((event) => (
              <Row
                event={event}
                key={event.id}
                local={local}
                onBook={setOpenEvent}
              />
            ))}
          </div>
        </section>
      ))}
    </div>
  );

  return (
    <>
      {/* Desktop: primary column (the CTA / featured session) beside a secondary
          column of other times. Collapses to a single column on narrow screens. */}
      {otherTimes ? (
        <div className="eap-layout">
          <div className="eap-col-primary">
            {featuredNode}
            {discord}
          </div>
          <div className="eap-col-secondary">
            <h2 className="eap-col-heading">Other times</h2>
            {otherTimes}
          </div>
        </div>
      ) : (
        <div className="eap-single">
          {featuredNode}
          {discord}
        </div>
      )}

      {openEvent && (
        <RegisterModal event={openEvent} onClose={() => setOpenEvent(null)} />
      )}
    </>
  );
}
