import Image from 'next/image';
import type { CSSProperties, ReactNode } from 'react';
import { InView, Rise } from './motion';

// "The Boundary stack": your data is the foundation, SQL is how everything reads it, and dashboards,
// metrics and alerts are layers on top. BAML sits underneath, free, and records every call.
// The source diagram's labels (e.g. "Your bucket", Lance tables) are placeholders, so the copy here is ours.

function Layer({
  i,
  title,
  body,
  className,
  children,
}: {
  i: number;
  title: string;
  body: string;
  className: string;
  children?: ReactNode;
}) {
  return (
    <div
      className={`stack-card flex flex-col gap-3 rounded-[12px] p-4 md:p-5 ${className}`}
      style={{ '--i': i } as CSSProperties}
    >
      <div className="flex flex-col gap-1">
        <span className="font-display font-medium text-[17px] leading-6 tracking-[-0.03em]">
          {title}
        </span>
        <span className="text-[13px] leading-[19px] opacity-75">{body}</span>
      </div>
      {children}
    </div>
  );
}

export default function Platform() {
  return (
    <section className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-30 pb-20 md:pb-34 border-t border-hair">
      <div className="flex flex-col lg:flex-row gap-10 lg:gap-20 lg:items-center">
        <div className="lg:w-[500px] shrink-0 flex flex-col gap-6">
          <Rise
            className="h2-display"
            text="Your data first. Everything else is a layer on top."
          />
          <p className="text-[17px] leading-[27px] text-ink-2">
            Every call your code makes lands in one place, hosted by Boundary or
            in your own AWS. SQL reads it, and the dashboards, metrics and
            alerts every platform has are built on top, including the ones your
            agent builds.
          </p>
        </div>

        <InView
          className="relative flex-1 min-w-0 rounded-[14px] overflow-hidden bg-pasture p-3 sm:p-6 md:p-10"
          light
        >
          <Image
            alt=""
            className="object-cover"
            fill
            quality={90}
            sizes="(min-width: 1024px) 760px, 100vw"
            src="/boundary/painting.jpg"
          />
          <div className="relative flex flex-col gap-2 max-w-[620px] mx-auto">
            <Layer
              body="The standard layers, from Studio dashboards to metrics and alerts, plus custom views your agent builds through the API."
              className="bg-white text-ink"
              i={3}
              title="Dashboards, metrics, alerts"
            />
            <Layer
              body="One query language for people and agents: Studio, baml query, curl, or an agent with a read key."
              className="bg-white text-ink"
              i={2}
              title="SQL over every call"
            />
            <Layer
              body="Every call, input, output and error, in one place. Hosted by Boundary, or in your own AWS."
              className="bg-pasture-800 text-white border border-white/10"
              i={1}
              title="Your data"
            />

            <div className="flex items-center gap-2 py-1.5 pl-5 font-mono text-xs leading-4 text-white">
              <span aria-hidden>↑</span> every call recorded
            </div>

            <Layer
              body="Free, runs on your machine and traced automatically, with no SDK. Python, TypeScript, Go, Rust, Java, Swift and C++."
              className="bg-clover-900 text-clover-50 border border-clover/20"
              i={0}
              title="BAML, open source"
            />
          </div>
        </InView>
      </div>
    </section>
  );
}
