import Link from 'next/link';

import { cn } from '@/lib/utils';

/** Muted link back to a parent view (legacy `.back-link`). */
export function BackLink({
  href,
  children,
}: {
  href: string;
  children: React.ReactNode;
}) {
  return (
    <Link
      href={href}
      className="text-sm text-muted-foreground no-underline hover:text-link"
    >
      {children}
    </Link>
  );
}

/**
 * Page header (legacy `header.page`): optional back link, the serifless h1,
 * and muted meta lines as children.
 */
export function PageHeader({
  back,
  title,
  className,
  children,
}: {
  back?: React.ReactNode;
  title: React.ReactNode;
  className?: string;
  children?: React.ReactNode;
}) {
  return (
    <header className={cn('mb-9 max-[640px]:mb-6', className)}>
      {back ? <p className="mb-1.5">{back}</p> : null}
      <h1 className="mb-1.5 text-[28px] font-medium tracking-[-0.01em] max-[640px]:text-[22px]">
        {title}
      </h1>
      <div className="text-[15px] leading-[1.55] text-muted-foreground max-[640px]:text-sm [&_strong]:font-semibold [&_strong]:text-foreground">
        {children}
      </div>
    </header>
  );
}
