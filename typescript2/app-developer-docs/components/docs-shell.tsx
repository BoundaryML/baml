import { ChevronRight } from 'lucide-react';
import Link from 'next/link';

import {
  DocsPageActions,
  DocsPageNavigation,
} from '@/components/docs-page-actions';
import { DocsSidebar } from '@/components/docs-sidebar';

interface BreadcrumbItem {
  href?: string;
  label: string;
}

interface TocItem {
  href: string;
  label: React.ReactNode;
}

export function DocsShell({
  breadcrumbs,
  children,
  description,
  title,
  toc = [],
}: {
  breadcrumbs: BreadcrumbItem[];
  children: React.ReactNode;
  description: string;
  title: string;
  toc?: TocItem[];
}) {
  return (
    <div className="container-wrapper flex flex-1 flex-col px-2">
      <div className="min-h-min flex-1 items-start px-0 [--sidebar-width:18rem] [--top-spacing:0rem] lg:grid lg:grid-cols-[var(--sidebar-width)_minmax(0,1fr)] lg:[--top-spacing:1rem]">
        <DocsSidebar />
        <div className="h-full w-full">
          <div
            className="flex scroll-mt-24 items-stretch pb-8 text-[1.05rem] sm:text-[15px] xl:w-full"
            data-slot="docs"
          >
            <div className="flex min-w-0 flex-1 flex-col">
              <div className="h-[var(--top-spacing)] shrink-0" />
              <article
                aria-label={breadcrumbs.map((item) => item.label).join(' / ')}
                className="mx-auto flex w-full max-w-160 min-w-0 flex-1 flex-col gap-6 px-4 py-6 text-foreground md:px-0 lg:py-8 dark:text-foreground"
              >
                <header className="flex flex-col gap-2">
                  {breadcrumbs.length > 1 ? (
                    <nav
                      aria-label="Breadcrumb"
                      className="flex items-center gap-1 text-sm text-muted-foreground"
                    >
                      {breadcrumbs.map((item, index) => (
                        <span
                          className="flex min-w-0 items-center gap-1"
                          key={`${item.label}-${index}`}
                        >
                          {index > 0 ? (
                            <ChevronRight
                              aria-hidden="true"
                              className="size-3.5 shrink-0"
                            />
                          ) : null}
                          {item.href ? (
                            <Link
                              className="truncate transition-colors hover:text-foreground"
                              href={item.href}
                            >
                              {item.label}
                            </Link>
                          ) : (
                            <span
                              aria-current="page"
                              className="truncate text-foreground"
                            >
                              {item.label}
                            </span>
                          )}
                        </span>
                      ))}
                    </nav>
                  ) : null}
                  <div className="flex items-center justify-between md:items-start">
                    <h1 className="scroll-m-24 text-3xl font-semibold tracking-tight sm:text-3xl">
                      {title}
                    </h1>
                    <DocsPageActions />
                  </div>
                  <p className="text-[1.05rem] text-muted-foreground sm:max-w-[80%] sm:text-balance sm:text-base">
                    {description}
                  </p>
                </header>
                <div
                  className="typeset w-full flex-1 pb-16 sm:pb-0"
                  data-docs-content=""
                >
                  {children}
                </div>
                <DocsPageNavigation />
              </article>
            </div>
            <aside
              aria-label="On this page"
              className="sticky top-[calc(var(--header-height)+1px)] z-30 ml-auto hidden h-[90svh] w-[var(--sidebar-width)] flex-col gap-4 overflow-hidden overscroll-none pb-8 xl:flex"
            >
              <div className="h-[var(--top-spacing)] shrink-0" />
              <div className="scroll-fade scrollbar-none flex flex-col gap-8 overflow-y-auto px-8">
                <nav className="flex flex-col gap-2 p-4 pt-0 text-sm">
                  <p className="h-6 bg-background text-xs font-medium text-muted-foreground">
                    On This Page
                  </p>
                  {toc.length ? (
                    toc.map((item) => (
                      <a
                        className="text-[0.8rem] text-muted-foreground no-underline transition-colors hover:text-foreground"
                        href={item.href}
                        key={item.href}
                      >
                        {item.label}
                      </a>
                    ))
                  ) : (
                    <span className="text-[0.8rem] text-muted-foreground">
                      Overview
                    </span>
                  )}
                </nav>
              </div>
              <div className="hidden flex-1 flex-col gap-6 px-6 xl:flex">
                <div className="group relative flex flex-col gap-2 rounded-2xl bg-surface p-6 text-sm text-surface-foreground">
                  <p className="text-balance text-base leading-tight font-semibold group-hover:underline">
                    Build with BAML
                  </p>
                  <p className="text-muted-foreground">
                    Start with the language guide and a checked BAML function.
                  </p>
                  <p className="text-muted-foreground">
                    Continue into generated package and CLI references for the
                    exact selected toolchain.
                  </p>
                  <Link
                    className="mt-2 inline-flex h-8 w-fit items-center rounded-lg border bg-background px-3 text-xs font-medium hover:bg-accent"
                    href="/baml/get-started"
                  >
                    Get started
                  </Link>
                </div>
              </div>
            </aside>
          </div>
        </div>
      </div>
    </div>
  );
}
