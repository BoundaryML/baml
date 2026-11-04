import type { ReactNode } from 'react';
import { section } from './analytics';
import { WRAP } from './ui';

/**
 * The top of an inner page (quickstart): the hero's headline type at a step smaller, a lead
 * in the hero's body size, and whatever actions the page puts under them.
 */
export default function PageHeader({
  title,
  lead,
  children,
}: {
  title: ReactNode;
  lead: ReactNode;
  children?: ReactNode;
}) {
  return (
    <section
      {...section('page_header')}
      aria-labelledby="page-title"
      className={`${WRAP} pt-14 xl:pt-[88px] pb-14 xl:pb-20 flex flex-col gap-8`}
    >
      <h1
        className="rise max-w-[900px] font-medium text-head text-[clamp(40px,4.2vw,60px)] max-sm:text-[clamp(34px,9.6vw,40px)] leading-[1.05] tracking-[-0.045em] text-balance"
        id="page-title"
      >
        {title}
      </h1>
      <p className="rise max-w-[600px] text-lg leading-[29px] text-body">
        {lead}
      </p>
      {children && (
        <div className="rise flex flex-wrap gap-2.5">{children}</div>
      )}
    </section>
  );
}
