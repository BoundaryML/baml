import Link from 'next/link';

export default function NotFoundPage() {
  return (
    <div className="container-wrapper flex-1">
      <div className="container grid min-h-[calc(100svh-var(--header-height)-5rem)] place-items-center px-6 py-16">
        <section className="relative w-full max-w-2xl overflow-hidden rounded-xl border bg-background p-8 shadow-sm sm:p-12">
          <div
            aria-hidden="true"
            className="absolute inset-x-0 top-0 h-px bg-gradient-to-r from-transparent via-[var(--docs-purple)] to-transparent"
          />
          <p className="text-sm font-medium text-[var(--docs-purple)]">
            Boundary developer documentation · Error 404
          </p>
          <h1 className="mt-3 max-w-xl text-balance text-3xl font-semibold tracking-tight sm:text-4xl">
            This documentation page does not exist.
          </h1>
          <p className="mt-4 max-w-xl leading-7 text-muted-foreground">
            We do not substitute another version or section when a route is
            missing. Choose a known documentation root or search from the
            header.
          </p>
          <div className="mt-7 flex flex-wrap gap-2">
            <Link className="button-primary docs-focus-ring" href="/">
              Return home
            </Link>
            <Link className="button-secondary docs-focus-ring" href="/baml">
              Browse BAML
            </Link>
          </div>
          <nav
            aria-label="404 recovery links"
            className="mt-8 flex flex-wrap gap-x-5 gap-y-2 border-t pt-5 text-sm text-muted-foreground"
          >
            <Link className="hover:text-foreground" href="/cli">
              CLI reference
            </Link>
            <Link className="hover:text-foreground" href="/tutorials">
              Tutorials
            </Link>
            <a
              className="hover:text-foreground"
              href="https://boundaryml.com/blog?tags=release"
            >
              Release notes
            </a>
          </nav>
        </section>
      </div>
    </div>
  );
}
