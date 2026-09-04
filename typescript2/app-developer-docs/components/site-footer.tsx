export function SiteFooter() {
  return (
    <footer className="bg-transparent">
      <div className="container-wrapper px-4 xl:px-6">
        <div className="flex h-[var(--footer-height)] items-center justify-between">
          <div className="w-full px-1 text-center text-xs leading-loose text-muted-foreground sm:text-sm">
            Developer documentation for BAML and Boundary products. Source
            available on{' '}
            <a
              className="font-medium underline underline-offset-4"
              href="https://github.com/BoundaryML/baml"
              rel="noreferrer"
              target="_blank"
            >
              GitHub
            </a>
            .{' '}
            <a
              className="hover:text-foreground"
              href="https://boundaryml.com/blog?tags=release"
            >
              View release notes
            </a>
          </div>
        </div>
      </div>
    </footer>
  );
}
