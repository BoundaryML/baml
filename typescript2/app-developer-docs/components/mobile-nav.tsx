'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useState } from 'react';

import {
  documentationNavigation,
  flattenDocumentationLinks,
  primaryNavigation,
} from '@/lib/navigation';

function MobileLink({
  href,
  label,
  onNavigate,
}: {
  href: string;
  label: string;
  onNavigate: () => void;
}) {
  const pathname = usePathname();
  const active =
    href === '/'
      ? pathname === '/'
      : pathname === href || pathname.startsWith(`${href}/`);

  return (
    <Link
      aria-current={active ? 'page' : undefined}
      className="docs-focus-ring flex items-center gap-2 rounded-sm text-2xl font-medium text-foreground data-[active=true]:underline data-[active=true]:underline-offset-4"
      data-active={active}
      href={href}
      onClick={onNavigate}
    >
      {label}
    </Link>
  );
}

export function MobileNav() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <button
        aria-expanded={open}
        aria-label="Toggle menu"
        className="docs-focus-ring extend-touch-target mr-4 inline-flex h-8 touch-manipulation items-center justify-start gap-2.5 rounded-sm p-0 text-foreground hover:bg-transparent lg:hidden"
        onClick={() => setOpen((current) => !current)}
        type="button"
      >
        <span className="relative flex h-8 w-4 items-center justify-center">
          <span aria-hidden="true" className="relative size-4">
            <span
              className={`absolute left-0 block h-0.5 w-4 bg-foreground transition-all duration-100 ${
                open ? 'top-[0.4rem] -rotate-45' : 'top-1'
              }`}
            />
            <span
              className={`absolute left-0 block h-0.5 w-4 bg-foreground transition-all duration-100 ${
                open ? 'top-[0.4rem] rotate-45' : 'top-2.5'
              }`}
            />
          </span>
        </span>
        <span className="flex h-8 items-center text-lg leading-none font-medium">
          Menu
        </span>
      </button>
      {open ? (
        <div className="fixed inset-x-0 top-[var(--header-height)] z-40 h-[calc(100svh-var(--header-height))] overflow-y-auto border-none bg-background/90 p-0 shadow-none backdrop-blur lg:hidden">
          <nav
            aria-label="Mobile navigation"
            className="flex flex-col gap-12 px-6 py-6"
          >
            <section className="flex flex-col gap-4">
              <h2 className="text-sm font-medium text-muted-foreground">
                Menu
              </h2>
              <div className="flex flex-col gap-3">
                {primaryNavigation.map((item) => (
                  <MobileLink
                    href={item.href}
                    key={item.href}
                    label={item.label}
                    onNavigate={() => setOpen(false)}
                  />
                ))}
              </div>
            </section>
            {documentationNavigation.map((group) => (
              <section className="flex flex-col gap-4" key={group.label}>
                <h2 className="text-sm font-medium text-muted-foreground">
                  {group.label === 'BAML' ? 'Sections' : group.label}
                </h2>
                <div className="flex flex-col gap-3">
                  {flattenDocumentationLinks(group.links).map((item) => (
                    <MobileLink
                      href={item.href}
                      key={item.href}
                      label={item.label}
                      onNavigate={() => setOpen(false)}
                    />
                  ))}
                </div>
              </section>
            ))}
          </nav>
        </div>
      ) : null}
    </>
  );
}
