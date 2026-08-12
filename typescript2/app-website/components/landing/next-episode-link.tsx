'use client';

import { ArrowRightIcon } from '@radix-ui/react-icons';
import Link from 'next/link';
import type { LumaEvent } from '@/lib/luma';

interface NextEpisodeLinkProps {
  className?: string;
  nextEvent?: LumaEvent | null;
}

export function NextEpisodeLink({
  className,
  nextEvent,
}: NextEpisodeLinkProps) {
  // Default to playground if no event or error
  const href = nextEvent?.url || '/playground';
  const isLiveEvent =
    nextEvent &&
    new Date(nextEvent.start_at) <= new Date() &&
    new Date(nextEvent.end_at) >= new Date();

  return (
    <Link href={href} target={nextEvent?.url ? '_blank' : '_self'}>
      <div
        className={`backdrop-filter-[12px] inline-flex w-full max-w-2xl items-center rounded-lg border border-border bg-accent/20 px-4 py-3 text-xs text-foreground transition-all ease-in hover:cursor-pointer hover:bg-accent/40 group translate-y-[-1rem] animate-fade-in opacity-0 ${className || ''}`}
      >
        <div className="flex flex-col sm:flex-row items-start sm:items-center gap-2 sm:gap-3 w-full">
          <div className="bg-gray-800 text-white px-2 py-1 rounded-md text-xs font-medium whitespace-nowrap flex-shrink-0 text-center w-full sm:w-auto">
            {isLiveEvent
              ? 'LIVE NOW'
              : nextEvent
                ? new Date(nextEvent.start_at).toLocaleDateString('en-US', {
                    day: 'numeric',
                    month: 'short',
                    weekday: 'short',
                  }) +
                  ' @ ' +
                  new Date(nextEvent.start_at).toLocaleTimeString('en-US', {
                    hour: 'numeric',
                    minute: '2-digit',
                    timeZoneName: 'short',
                  })
                : 'TH JUL 31 @ 9 AM PT'}
          </div>
          <div className="flex-1 min-w-0 flex items-center justify-between sm:justify-start gap-2">
            <span className="text-sm font-medium leading-tight block">
              {isLiveEvent
                ? 'Join BAML Session'
                : nextEvent
                  ? nextEvent.name || 'BAML Podcast'
                  : 'Try BAML online'}
            </span>
            <ArrowRightIcon className="size-4 transition-transform duration-300 ease-in-out group-hover:translate-x-0.5 flex-shrink-0 sm:hidden" />
          </div>
          <ArrowRightIcon className="size-4 transition-transform duration-300 ease-in-out group-hover:translate-x-0.5 flex-shrink-0 hidden sm:block" />
        </div>
      </div>
    </Link>
  );
}
