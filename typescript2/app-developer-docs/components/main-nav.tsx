'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

import { primaryNavigation } from '@/lib/navigation';

export function MainNav() {
  const pathname = usePathname();

  return (
    <nav
      aria-label="Primary navigation"
      className="hidden items-center gap-0 lg:flex"
    >
      {primaryNavigation.map((item) => (
        <Link
          className="relative inline-flex h-8 items-center rounded-md px-2.5 text-sm font-medium text-foreground transition-colors hover:bg-accent"
          data-active={
            item.href === '/'
              ? pathname === '/'
              : pathname === item.href || pathname.startsWith(`${item.href}/`)
          }
          href={item.href}
          key={item.href}
        >
          {item.label}
        </Link>
      ))}
    </nav>
  );
}
