'use client';

import { Menu, X } from 'lucide-react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useId, useRef, useState } from 'react';

import { DocsNavigationTree } from '@/components/docs-sidebar';
import { documentationNavigation, primaryNavigation } from '@/lib/navigation';

export function MobileNav() {
  const [open, setOpen] = useState(false);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const pathname = usePathname();
  const dialogId = useId();
  const titleId = useId();

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (!open) {
      dialog.close();
      return;
    }

    dialog.showModal();
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const desktop = window.matchMedia('(min-width: 1024px)');
    const closeOnDesktop = () => {
      if (desktop.matches) setOpen(false);
    };
    desktop.addEventListener('change', closeOnDesktop);
    closeOnDesktop();
    return () => {
      document.body.style.overflow = previousOverflow;
      desktop.removeEventListener('change', closeOnDesktop);
    };
  }, [open]);

  const close = () => setOpen(false);

  return (
    <>
      <button
        aria-controls={dialogId}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label="Open navigation"
        className="docs-focus-ring mr-4 inline-flex h-9 items-center gap-2 rounded-md text-sm font-medium text-foreground lg:hidden"
        onClick={() => setOpen(true)}
        type="button"
      >
        <Menu aria-hidden="true" className="size-4" />
        Menu
      </button>
      <dialog
        aria-labelledby={titleId}
        className="mobile-nav-dialog"
        id={dialogId}
        onClose={close}
        ref={dialogRef}
      >
        <button
          aria-label="Close navigation backdrop"
          className="mobile-nav-backdrop"
          onClick={close}
          tabIndex={-1}
          type="button"
        />
        <div className="mobile-nav-panel">
          <div className="mobile-nav-header">
            <h2 id={titleId}>Documentation</h2>
            <button
              aria-label="Close navigation"
              className="docs-focus-ring mobile-nav-close"
              onClick={close}
              type="button"
            >
              <X aria-hidden="true" className="size-4" />
            </button>
          </div>
          <nav aria-label="Mobile navigation" className="mobile-nav-content">
            <div className="mobile-nav-primary">
              {primaryNavigation.map((item) => {
                const active =
                  item.href === '/'
                    ? pathname === '/'
                    : pathname === item.href ||
                      pathname.startsWith(`${item.href}/`);
                return (
                  <Link
                    aria-current={active ? 'location' : undefined}
                    className="docs-focus-ring"
                    data-active={active}
                    href={item.href}
                    key={item.href}
                    onClick={close}
                  >
                    {item.label}
                  </Link>
                );
              })}
            </div>
            {documentationNavigation.map((group) => (
              <section className="mobile-nav-section" key={group.label}>
                <h3>
                  {group.label === 'BAML' ? 'BAML documentation' : group.label}
                </h3>
                <DocsNavigationTree
                  links={group.links}
                  onNavigate={close}
                  storageId={`mobile:${group.label}`}
                  variant="mobile"
                />
              </section>
            ))}
          </nav>
        </div>
      </dialog>
    </>
  );
}
