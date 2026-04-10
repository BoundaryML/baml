'use client';

import Link from 'next/link';
import { siteConfig } from '@/app/_lib/config';
import { SectionHeader } from '@/components/section-header';
import { Button } from '@/components/ui/button';

export function StorySection() {
  const { title, teaser, timeline, ctaText, ctaHref } = siteConfig.storySection;

  return (
    <section
      className="flex flex-col items-center justify-center w-full relative px-2 md:px-10"
      id="story"
    >
      <div className="mx-2 md:mx-10 relative w-full max-w-4xl">
        <SectionHeader>
          <h2 className="text-3xl md:text-4xl font-medium tracking-tighter text-center text-balance pb-1">
            {title}
          </h2>
          <p className="text-muted-foreground text-center text-balance font-medium max-w-2xl">
            {teaser}
          </p>
        </SectionHeader>

        {/* Timeline strip */}
        <div className="mt-10 rounded-xl border border-border bg-card/40 p-6 sm:p-8 backdrop-blur-sm">
          <div className="flex flex-wrap items-center justify-center gap-x-6 gap-y-4 sm:gap-x-8">
            {timeline.map((node, i) => (
              <div
                className="flex items-center gap-2 sm:gap-3"
                key={node.era}
              >
                <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                  {node.era}
                </span>
                <span className="font-semibold text-foreground">
                  {node.label}
                </span>
                {i < timeline.length - 1 && (
                  <span
                    aria-hidden
                    className="hidden text-muted-foreground/40 sm:inline"
                  >
                    →
                  </span>
                )}
              </div>
            ))}
          </div>
          <div className="mt-6 flex justify-center border-t border-border pt-6">
            <Button asChild variant="outline" size="lg">
              <Link href={ctaHref}>{ctaText}</Link>
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
